import QtQuick
import QtCore
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtQuick.Shapes
import org.kde.kirigami as Kirigami
import org.w3cdotorg.ressac

Kirigami.ApplicationWindow {
    id: root

    title: "Ressac"
    width: 1100
    height: 720
    minimumWidth: 960
    minimumHeight: 500
    visible: true

    // The Sea Chart: a window of its own, made on demand and dropped when closed.
    Loader { id: chartLoader; active: false; sourceComponent: Component { SeaChartWindow { chrome: root } } }
    function openChart() {
        chartLoader.active = true
        const w = chartLoader.item
        w.show(); w.raise(); w.requestActivate()
        Chart.openWindow()
    }
    function chartClosed() { chartLoader.active = false }
    // Web addresses go to the user's browser; a walk only logs them.
    function openExternally(url) {
        if (Recette.active)
            console.warn("open: " + url)
        else
            Qt.openUrlExternally(url)
    }
    Binding { target: mainMenu.seaChart; property: "enabled"; value: true }
    Connections { target: mainMenu.seaChart; function onTriggered() { root.openChart() } }
    Connections {
        target: Chart
        function onOpenFeed(key) {
            SourcesModel.selectKey(key)
            if (root.visibility === Window.Minimized) root.showNormal()
            root.show(); root.raise(); root.requestActivate()
        }
        function onOpenUrl(url) { root.openExternally(url) }
    }

    // Add Feed, OPML import and export (B6d): the menu actions' state and triggers come from here.
    function addFeed() { Library.beginAdd() }
    function importOpml() { opml.chooseImport() }
    function exportOpml() { opml.chooseExport() }
    Binding { target: mainMenu.addFeed; property: "enabled"; value: Launcher.phase === Launcher.Ready }
    Binding { target: mainMenu.importOpml; property: "enabled"; value: Launcher.phase === Launcher.Ready && !Library.busy }
    Binding { target: mainMenu.exportOpml; property: "enabled"; value: Launcher.phase === Launcher.Ready }
    Connections { target: mainMenu.addFeed; function onTriggered() { root.addFeed() } }
    Connections { target: mainMenu.importOpml; function onTriggered() { root.importOpml() } }
    Connections { target: mainMenu.exportOpml; function onTriggered() { root.exportOpml() } }

    SettingsWindow { id: settingsWindow; chrome: root }
    function openSettings() { settingsWindow.show(); settingsWindow.raise(); settingsWindow.requestActivate() }
    Binding { target: mainMenu.settings; property: "enabled"; value: true }
    Connections { target: mainMenu.settings; function onTriggered() { root.openSettings() } }
    Connections { target: Library; function onAdded(key) { SourcesModel.selectKey(key) } }
    // `reloadAll` reaches the form's pop-ups: a database change under an open dialog.
    Connections { target: Runs; function onDatabaseChanged() { Library.reloadChoices() } }
    AddFeedDialog { id: addFeedDialog; onOpenUrl: (url) => root.openExternally(url) }
    OpmlDialogs { id: opml }
    TextEdit { id: clipboardProbe; visible: false }          // walk only: puts text on the clipboard

    // The note under the window: what an add did, gone after Library.noteSeconds or on Dismiss.
    footer: Rectangle {
        visible: Library.note !== ""
        implicitHeight: visible ? noteRow.implicitHeight + Kirigami.Units.smallSpacing * 2 : 0
        color: Kirigami.Theme.backgroundColor
        Kirigami.Theme.colorSet: Kirigami.Theme.Window
        RowLayout {
            id: noteRow
            anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter
                leftMargin: Kirigami.Units.largeSpacing; rightMargin: Kirigami.Units.smallSpacing }
            spacing: Kirigami.Units.smallSpacing
            Kirigami.Icon { source: "emblem-ok-symbolic"; color: Kirigami.Theme.positiveTextColor
                implicitWidth: Kirigami.Units.iconSizes.small; implicitHeight: Kirigami.Units.iconSizes.small }
            QQC2.Label { Layout.fillWidth: true; text: Library.note; maximumLineCount: 2; wrapMode: Text.Wrap; textFormat: Text.PlainText }
            QQC2.ToolButton { icon.name: "window-close"; display: QQC2.AbstractButton.IconOnly; text: "Dismiss"; onClicked: Library.dismissNote() }
        }
        Timer {
            id: noteTimer
            interval: Library.noteSeconds * 1000
            running: Library.note !== ""
            onTriggered: Library.dismissNote()
        }
        Connections { target: Library; function onNoteChanged() { if (Library.note !== "") noteTimer.restart() } }
    }

    function useDefaultDatabase() {
        AppSettings.useDefaultLocation()
        Launcher.relaunch()
    }

    // --screenshot (spec §8.2): grab the settled launch screen, then each walk step (ready only), then quit.
    readonly property var walkSteps: [           // { name: "02-…", enter: function () { … } }, ready only
        { name: "02-folder-blogs", enter: function () { SourcesModel.selectKey("folder:1"); SourcesModel.toggleExpanded(SourcesModel.rowOfKey("folder:1"))
            console.warn("walk: 02-folder-blogs entries=" + EntriesModel.rowCount() + " expanded=" + (SourcesModel.rowOfKey("feed:1") >= 0)) } },
        { name: "03-starred", enter: function () { EntriesModel.select(0); EntriesModel.toggleStarred(0)
            console.warn("walk: 03-starred selected=" + EntriesModel.selectedEntryId) } },
        { name: "04-search", enter: function () { EntriesModel.searchText = "comic"; console.warn("walk: 04-search entries=" + EntriesModel.rowCount()) } },
        { name: "05-banner", enter: function () { Runs.showBanner("The database is busy (is `ressac` running in a terminal?)", false)
            console.warn("walk: 05-banner visible=" + Runs.bannerVisible) } },
        { name: "06-garden", enter: function () { EntriesModel.searchText = ""; SourcesModel.selectKey("garden:needs-triage")
            console.warn("walk: 06-garden active=" + EntriesModel.active + " unreadEnabled=" + !SourcesModel.isGarden) } },
        { name: "07-unread-filter", enter: function () {
            const page = readingLoader.item
            SourcesModel.selectKey("unread"); page.unreadButton.click()          // a user click on Unread ...
            const clicked = EntriesModel.unreadOnly
            EntriesModel.unreadOnly = false                                              // ... the model moves on its own
            console.warn("walk: 07-unread-filter clicked=" + clicked + " all=" + page.allButton.checked + " unread=" + page.unreadButton.checked)
            const menu = page.sourcesPane.feedMenu
            const row = { feedId: 1, key: "feed:1", badge: 0, site: "", preferFullText: true }
            menu.prepare(row); const first = menu.fullTextItem.checked
            menu.fullTextItem.checked = false                                            // the user unticks it ...
            menu.prepare(row); const again = menu.fullTextItem.checked                   // ... and reopens on a ticked feed
            menu.prepare({ feedId: 2, key: "feed:2", badge: 3, site: "x", preferFullText: false })
            console.warn("walk: 07-feed-menu first=" + first + " again=" + again + " other=" + menu.fullTextItem.checked + " feedId=" + menu.feedId + " badge=" + menu.badge) } },
        { name: "08-full-article", enter: function () { SourcesModel.selectKey("feed:8"); EntriesModel.select(0)
            console.warn("walk: 08-full-article entry=" + Reader.entryId + " showing=" + Reader.isShowingFullText) } },
        { name: "09-feed-text", enter: function () { Reader.requestFullText(); console.warn("walk: 09-feed-text showing=" + Reader.isShowingFullText) } },
        { name: "10-link", enter: function () { readingLoader.item.reader.clickTitleLink() } },
        { name: "11-extraction", enter: function () { EntriesModel.select(1); Reader.requestFullText()
            console.warn("walk: 11-extraction entry=" + Reader.entryId + " extracting=" + Reader.isExtracting) } },
        { name: "12-notice", enter: function () { console.warn("walk: 12-notice showing=" + Reader.isShowingFullText + " help=" + Reader.fullTextHelp) } },
        // Keys go through Recette.key (the window filter, then the page's perform), as a user's would.
        { name: "13-j-in-sidebar", enter: function () { SourcesModel.selectKey("feed:1"); readingLoader.item.focusSidebar(); Recette.key(Qt.Key_J, "j", 0)
            console.warn("walk: 13-j-in-sidebar selected=" + EntriesModel.selectedEntryId + " focus=" + readingLoader.item.focusName()) } },
        { name: "14-arrows-in-list", enter: function () { Recette.key(Qt.Key_Right, "", 0); Recette.key(Qt.Key_Down, "", 0)
            console.warn("walk: 14-arrows-in-list selected=" + EntriesModel.selectedEntryId + " focus=" + readingLoader.item.focusName()) } },
        { name: "15-space-in-reader", enter: function () { readingLoader.item.reader.focusWeb(); Recette.key(Qt.Key_Space, " ", 0) } },
        { name: "16-text-field", enter: function () { console.warn("walk: 15-space-in-reader selected=" + EntriesModel.selectedEntryId)
            readingLoader.item.focusSearch(); Recette.key(Qt.Key_J, "j", 0)
            console.warn("walk: 16-text-field search=" + EntriesModel.searchText + " selected=" + EntriesModel.selectedEntryId) } },
        { name: "17-garden-keys", enter: function () { EntriesModel.searchText = ""; SourcesModel.selectKey("garden:needs-triage"); readingLoader.item.focusSidebar(); Recette.key(Qt.Key_K, "k", 0)
            console.warn("walk: 17-garden-keys verb=" + readingLoader.item.lastVerb + " handled=" + readingLoader.item.lastHandled) } },
        { name: "18-auto-read", enter: function () { SourcesModel.selectKey("starred"); EntriesModel.select(1)
            console.warn("walk: 18-auto-read selected=" + EntriesModel.selectedEntryId + " read=" + EntriesModel.selectedRead + " page=" + Reader.pageUrl) } },
        { name: "19-auto-read-wait-a", enter: function () {} },
        { name: "20-auto-read-wait-b", enter: function () {} },
        { name: "21-auto-read-done", enter: function () { console.warn("walk: 21-auto-read-done read=" + EntriesModel.selectedRead + " page=" + Reader.pageUrl) } },
        // The reader (WebEngine's own item) has the focus: k, j, m, m, s, s still reach the router first.
        { name: "22-reader-keys", enter: function () {
            const page = readingLoader.item
            page.reader.focusWeb()
            const focus = page.focusName() + " item=" + String(root.activeFocusItem).split("(")[0]
            Recette.key(Qt.Key_K, "k", 0); const k = EntriesModel.selectedEntryId
            Recette.key(Qt.Key_J, "j", 0); const j = EntriesModel.selectedEntryId
            Recette.key(Qt.Key_M, "m", 0); const m = EntriesModel.selectedRead
            Recette.key(Qt.Key_M, "m", 0); const m2 = EntriesModel.selectedRead
            Recette.key(Qt.Key_S, "s", 0); const s = EntriesModel.selectedStarred
            Recette.key(Qt.Key_S, "s", 0); const s2 = EntriesModel.selectedStarred
            console.warn("walk: 22-reader-keys focus=" + focus + " k=" + k + " j=" + j + " m=" + m + " m2=" + m2 + " s=" + s + " s2=" + s2) } },
        { name: "23-menu", enter: function () { mainMenu.openMenu("article")
            console.warn("walk: 23-menu read=" + mainMenu.toggleRead.text + " enabled=" + mainMenu.toggleRead.enabled + " star=" + mainMenu.toggleStar.text) } },
        { name: "24-text-size", enter: function () { mainMenu.closeMenus(); const before = AppSettings.readerSize
            mainMenu.biggerText.trigger(); const bigger = AppSettings.readerSize
            mainMenu.actualSize.trigger(); console.warn("walk: 24-text-size bigger=" + bigger + " actual=" + AppSettings.readerSize)
            AppSettings.readerSize = before } },         // the walk leaves the setting as it found it
        // The fixture: edition 1 built 2026-10-10T08:00Z with 1 unreachable feed; This Week has entries 5 and 6
        // (Daring Fireball), The Wire entry 1 (xkcd.com) then "+2 more in xkcd.com".
        { name: "23-journal", enter: function () { SourcesModel.selectKey("journal")
            console.warn("walk: 23-journal rows=" + EntriesModel.rowCount() + " phase=" + Journal.phase + " placeholder=" + Journal.placeholder
                + " header=" + Journal.header + " focus=" + readingLoader.item.focusName()) } }
    ]
    // The `journal` walk: the test points every active feed at a closed loopback port, and the launch trigger builds at once.
    readonly property var journalSteps: [
        { name: "02-journal", enter: function () { SourcesModel.selectKey("journal"); console.warn("walk: 02-journal phase=" + Journal.phase) } },
        { name: "03-wait", enter: function () {} },
        { name: "04-wait", enter: function () {} },
        { name: "05-offline", enter: function () { console.warn("walk: 05-offline phase=" + Journal.phase + " placeholder=" + Journal.placeholder
            + " retry=" + Journal.offersRetry + " banner=" + Runs.bannerMessage) } },
        { name: "06-retry", enter: function () { Journal.retry(); console.warn("walk: 06-retry phase=" + Journal.phase) } },
        { name: "07-wait", enter: function () {} },
        { name: "08-retried", enter: function () { const after = Journal.phase; Journal.trigger()
            console.warn("walk: 08-retried phase=" + after + " after-trigger=" + Journal.phase) } },
        // A fetch in flight turns the build away: the edition stays due, and Retry builds it once the fetch is over.
        { name: "09-busy", enter: function () { Runs.refreshAll(); Journal.retry(); console.warn("walk: 09-busy active=" + Runs.longRunActive) } },
        { name: "10-wait", enter: function () {} },
        { name: "11-refused", enter: function () { console.warn("walk: 11-refused phase=" + Journal.phase + " retry=" + Journal.offersRetry + " active=" + Runs.longRunActive) } },
        { name: "12-retry-again", enter: function () { Journal.retry(); console.warn("walk: 12-retry-again phase=" + Journal.phase) } },
        { name: "13-wait", enter: function () {} },
        { name: "14-offline-again", enter: function () { console.warn("walk: 14-offline-again phase=" + Journal.phase) } }
    ]
    // The `mark-all` walk: on a fresh copy of the fixture, Mark All as Read in the Journal marks the edition's
    // entries (1, 5, 6) and never the entries outside it (the overflow's hidden ones, 3 and 4).
    readonly property var markAllSteps: [
        { name: "02-journal", enter: function () { SourcesModel.selectKey("journal")
            console.warn("walk: 02-journal rows=" + EntriesModel.rowCount() + " can=" + EntriesModel.canMarkAllRead) } },
        { name: "03-mark-all", enter: function () { EntriesModel.markAllRead()
            console.warn("walk: 03-mark-all can=" + EntriesModel.canMarkAllRead) } },
        { name: "04-marked", enter: function () { console.warn("walk: 04-marked rows=" + EntriesModel.rowCount() + " header=" + Journal.header) } }
    ]
    // The `garden` walk (fixture: queue Hijacked 6, Moved 5 (a safe repair), Fossil 2; Unphotographable 7 kept; feed 4 buried;
    // the test adds the themes Literature and Technology, feed 8 on Technology).
    readonly property var gardenSteps: [
        { name: "02-triage", enter: function () { SourcesModel.selectKey("garden:needs-triage")
            console.warn("walk: 02-triage count=" + Garden.countLabel + " selected=" + Garden.selectedFeedId + " rows=" + Garden.rowCount()
                + " apply=" + Garden.applyTitle + " focus=" + readingLoader.item.focusName() + " sections=" + Garden.sectionTitles.join("|") + " current=" + readingLoader.item.garden.currentRow + " row=" + Garden.selectedRow) } },
        { name: "03-keep", enter: function () { Recette.key(Qt.Key_K, "k", 0) } },
        { name: "04-wait", enter: function () {} },
        { name: "05-kept", enter: function () { console.warn("walk: 05-kept count=" + Garden.countLabel + " selected=" + Garden.selectedFeedId
            + " rows=" + Garden.rowCount() + " kept6=" + (Garden.rowOfFeed(6) >= 0) + " current=" + readingLoader.item.garden.currentRow + " row=" + Garden.selectedRow) } },
        { name: "06-confirm", enter: function () { Recette.key(Qt.Key_R, "R", Qt.ShiftModifier)
            console.warn("walk: 06-confirm confirming=" + Garden.confirming + " question=" + Garden.applyQuestion
                + " preview=" + Garden.applyPreview + " confirm=" + Garden.applyConfirmLabel) } },
        { name: "07-apply", enter: function () { readingLoader.item.garden.acceptRepairs() } },
        { name: "08-wait", enter: function () {} },
        { name: "09-applied", enter: function () { console.warn("walk: 09-applied count=" + Garden.countLabel + " selected=" + Garden.selectedFeedId + " current=" + readingLoader.item.garden.currentRow + " row=" + Garden.selectedRow) } },
        { name: "10-undo", enter: function () { Recette.key(Qt.Key_U, "u", 0) } },
        { name: "11-wait", enter: function () {} },
        { name: "12-undone", enter: function () { console.warn("walk: 12-undone count=" + Garden.countLabel + " selected=" + Garden.selectedFeedId + " current=" + readingLoader.item.garden.currentRow + " row=" + Garden.selectedRow) } },
        { name: "13-all-rhythm", enter: function () { SourcesModel.selectKey("garden:all"); Garden.grouping = "rhythm"
            console.warn("walk: 13-all-rhythm count=" + Garden.countLabel + " sections=" + Garden.sectionTitles.join("|")) } },
        { name: "14-inspector", enter: function () { Garden.selectFeed(8)
            console.warn("walk: 14-inspector rhythmIndex=" + Garden.rhythmIndex + " theme=" + Garden.themeIndex
                + " themes=" + Garden.themeNames.join("|") + " rhythm=" + Garden.rhythmLine) } },
        { name: "15-set", enter: function () { Garden.setCadence(1); Garden.setTheme(1) } },
        { name: "16-wait", enter: function () {} },
        { name: "17-set-done", enter: function () { console.warn("walk: 17-set-done rhythmIndex=" + Garden.rhythmIndex + " theme=" + Garden.themeIndex
            + " rhythm=" + Garden.rhythmLine) } },
        { name: "18-open", enter: function () { readingLoader.item.focusGarden(); Recette.key(Qt.Key_O, "o", 0) } },
        { name: "19-cemetery", enter: function () { SourcesModel.selectKey("garden:cemetery")
            console.warn("walk: 19-cemetery count=" + Garden.countLabel + " selected=" + Garden.selectedFeedId + " keep=" + Garden.keepTitle
                + " apply=" + Garden.showsApply) } },
        { name: "20-buried-key", enter: function () { Recette.key(Qt.Key_B, "b", 0); console.warn("walk: 20-buried-key message=" + Garden.message) } }
    ]
    // The `chart` and `chart-live` walks: the Sea Chart window (B7c).
    readonly property var chartSteps: [
        { name: "02-open", enter: function () { root.openChart()
            console.warn("walk: 02-open open=" + Chart.open + " reduce=" + Chart.reduceMotion + " onScreen=" + Live.chartOnScreen) } },
        { name: "03-wait", enter: function () {} }, { name: "04-wait", enter: function () {} }, { name: "05-wait", enter: function () {} },
        { name: "06-wait", enter: function () {} }, { name: "07-wait", enter: function () {} }, { name: "08-wait", enter: function () {} },
        { name: "09-wait", enter: function () {} },
        { name: "10-first", enter: function () { chartLoader.item.probe("10-first") } },
        { name: "11-fixture", enter: function () { const home = String(StandardPaths.writableLocation(StandardPaths.HomeLocation))
            console.warn("walk: 11-fixture shown=" + Chart.showFile(home + "/fixture.json")) } },
        { name: "12-wait", enter: function () {} }, { name: "13-wait", enter: function () {} }, { name: "14-wait", enter: function () {} },
        { name: "15-drawn", enter: function () { chartLoader.item.probe("15-drawn") } },
        { name: "16-messages", enter: function () { chartLoader.item.runPage(
            "const h = window.webkit.messageHandlers.ressac;"
            + "h.postMessage({type: 'openFeed', id: 8}); h.postMessage({type: 'openURL', url: 'file:///etc/passwd'});"
            + "h.postMessage({type: 'openURL', url: 'javascript:alert(1)'}); h.postMessage({type: 'openFeed', id: '9'});"
            + "h.postMessage({type: 'openURL', url: 'https://example.invalid/x'});"
            + "[...document.querySelectorAll('.bar button')].find(b => b.textContent === 'From the harbour').click();") } },
        { name: "17-wait", enter: function () {} },
        { name: "18-answered", enter: function () { console.warn("walk: 18-answered selected=" + SourcesModel.selectedKey + " view=" + AppSettings.chartView
            + " ink=" + AppSettings.chartInk + " animate=" + AppSettings.chartAnimate); chartLoader.item.requestActivate() } },
        { name: "19-navigate", enter: function () { chartLoader.item.runPage(
            "for (const t of ['LICENSE-d3.txt', 'https://example.invalid/']) { const a = document.createElement('a'); a.href = t; document.body.append(a); a.click() }"
            + "setTimeout(() => { location.href = 'about:blank' }, 100);") } },
        { name: "20-wait", enter: function () {} }, { name: "21-wait", enter: function () {} }, { name: "22-wait", enter: function () {} },
        { name: "23-home", enter: function () { chartLoader.item.probe("23-home") } },
        { name: "24-close", enter: function () { chartLoader.item.width = 1200; chartLoader.item.close() } },
        { name: "25-closed", enter: function () { console.warn("walk: 25-closed open=" + Chart.open + " onScreen=" + Live.chartOnScreen
            + " window=" + (chartLoader.item !== null)) } },
    ]
    readonly property var chartLiveSteps: [
        { name: "02-open", enter: function () { root.openChart() } },
        { name: "03-wait", enter: function () {} }, { name: "04-wait", enter: function () {} }, { name: "05-wait", enter: function () {} },
        { name: "06-first", enter: function () { console.warn("walk: 06-first surveys=" + Chart.surveys + " phase=" + Chart.phase) } },
        { name: "07-write", enter: function () { root.requestActivate(); SourcesModel.selectKey("unread"); EntriesModel.select(0); EntriesModel.toggleStarred(0) } },
        { name: "08-wait", enter: function () {} }, { name: "09-wait", enter: function () {} }, { name: "10-wait", enter: function () {} },
        { name: "11-wait", enter: function () {} }, { name: "12-wait", enter: function () {} }, { name: "13-wait", enter: function () {} },
        { name: "14-wait", enter: function () {} },
        { name: "15-second", enter: function () { console.warn("walk: 15-second surveys=" + Chart.surveys) } },
        { name: "16-retry", enter: function () { Chart.retry() } },
        { name: "17-wait", enter: function () {} },
        { name: "18-third", enter: function () { console.warn("walk: 18-third surveys=" + Chart.surveys) } },
        { name: "19-close", enter: function () { chartLoader.item.close() } },
        { name: "20-note", enter: function () { console.warn("walk: 20-note surveys=" + Chart.surveys); Chart.noteChange(); EntriesModel.toggleStarred(0) } },
        { name: "21-wait", enter: function () {} }, { name: "22-wait", enter: function () {} }, { name: "23-wait", enter: function () {} },
        { name: "24-wait", enter: function () {} }, { name: "25-wait", enter: function () {} }, { name: "26-wait", enter: function () {} },
        { name: "27-wait", enter: function () {} },
        { name: "28-after", enter: function () { console.warn("walk: 28-after surveys=" + Chart.surveys + " open=" + Chart.open
            + " onScreen=" + Live.chartOnScreen + " polling=" + Live.polling) } },
    ]
    // The `library` walk (B6d): an empty library (the launch creates the file), a dropped OPML, the add dialog.
    // Every address targets a closed loopback port or is refused before any connection.
    readonly property var librarySteps: [
        { name: "02-first-run", enter: function () { console.warn("walk: 02-first-run empty=" + SourcesModel.emptyLibrary
            + " add=" + mainMenu.addFeed.enabled + " import=" + mainMenu.importOpml.enabled + " page=" + (mainMenu.page === null)) } },
        { name: "03-drop", enter: function () { const home = String(StandardPaths.writableLocation(StandardPaths.HomeLocation))
            console.warn("walk: 03-drop started=" + Library.openUrls([home + "/notes.txt", home + "/walk.opml"]) + " again="
                + Library.openUrls([home + "/walk.opml"]) + " busy=" + Library.busy) } },
        { name: "04-wait", enter: function () {} },
        { name: "05-confirm", enter: function () { console.warn("walk: 05-confirm step=" + Library.opmlStep + " title=" + Library.importTitle
            + " lines=" + Library.importLines.join("|") + " note=" + Library.importNote + " canImport=" + Library.canImport) } },
        { name: "06-import", enter: function () { opml.confirmImport() } },
        { name: "07-wait", enter: function () {} },
        { name: "08-wait", enter: function () {} },
        { name: "09-wait", enter: function () {} },
        { name: "10-imported", enter: function () { console.warn("walk: 10-imported empty=" + SourcesModel.emptyLibrary + " step=" + Library.opmlStep
            + " walk=" + (SourcesModel.rowOfKey("folder:1") >= 0) + " gamma=" + (SourcesModel.rowOfKey("feed:3") >= 0) + " longRun=" + Runs.longRunActive) } },
        { name: "11-add-dialog", enter: function () { clipboardProbe.text = "  https://example.org/feed.xml\n"; clipboardProbe.selectAll(); clipboardProbe.copy()
            root.addFeed(); console.warn("walk: 11-add-dialog open=" + Library.addOpen + " address=" + Library.address + " themes=" + Library.themeNames.join("|")
                + " folders=" + Library.folderTitles.join("|") + " canAdd=" + Library.canAdd) } },
        { name: "12-add-refused", enter: function () { Library.address = "http://127.0.0.1:9/feed"; addFeedDialog.accept() } },
        { name: "13-wait", enter: function () {} },
        { name: "14-add-refusal", enter: function () { console.warn("walk: 14-add-refusal error=" + Library.addError + " showLog=" + Library.addShowsLog
            + " open=" + Library.addOpen + " adding=" + Library.adding) } },
        { name: "15-add-known", enter: function () { Library.address = "http://127.0.0.21:9/alpha.xml"; Library.folderIndex = 1; addFeedDialog.accept() } },
        { name: "16-wait", enter: function () {} },
        { name: "17-add-known", enter: function () { console.warn("walk: 17-add-known error=" + Library.addError + " showLog=" + Library.addShowsLog)
            Library.cancelAdd(); console.warn("walk: 17-closed open=" + Library.addOpen) } },
        { name: "18-export", enter: function () { const home = String(StandardPaths.writableLocation(StandardPaths.HomeLocation))
            console.warn("walk: 18-export started=" + Library.exportTo(home + "/" + Library.exportName) + " refused=" + Library.exportTo("https://example.org/x.opml")) } },
        { name: "19-wait", enter: function () {} },
        { name: "20-drag", enter: function () { SourcesModel.toggleExpanded(SourcesModel.rowOfKey("folder:1")); SourcesModel.selectKey("feed:2") } },
        { name: "21-drag-root", enter: function () { console.warn("walk: 21-drag moved=" + readingLoader.item.dragFeed("feed:2", "heading:Feeds")
            + " ignored=" + readingLoader.item.dragFeed("feed:3", "heading:Feeds") + " own=" + readingLoader.item.dragFeed("feed:1", "folder:1")
            + " notFeed=" + readingLoader.item.dragFeed("folder:1", "heading:Feeds")) } },
        { name: "22-wait", enter: function () {} },
        { name: "23-dragged", enter: function () { console.warn("walk: 23-dragged selected=" + SourcesModel.selectedKey)
            console.warn("walk: 23-drag-in moved=" + readingLoader.item.dragFeed("feed:3", "folder:1")) } },
        { name: "24-wait", enter: function () {} },
        { name: "25-entry-drag", enter: function () { const data = Library.entryDragData("https://example.org/a", "A title")
            console.warn("walk: 25-entry-drag plain=" + data["text/plain"] + " moz=" + JSON.stringify(data["text/x-moz-url"])
                + " none=" + Object.keys(Library.entryDragData("javascript:alert(1)", "x")).length) } },
        { name: "26-settings", enter: function () { root.openSettings()
            console.warn("walk: 26-settings fonts=" + AppSettings.fontTitles.join("|") + " widths=" + AppSettings.widthTitles.join("|")
                + " registered=" + ["Newsreader", "Atkinson Hyperlegible Next"].every(f => Qt.fontFamilies().includes(f))) } },
        { name: "27-data", enter: function () { settingsWindow.showPage("data")
            console.warn("walk: 27-data path=" + AppSettings.resolvedDatabasePath + " note=" + AppSettings.databaseNote + " ressacDb=" + AppSettings.ressacDbSet) } },
        { name: "28-junk", enter: function () { const home = String(StandardPaths.writableLocation(StandardPaths.HomeLocation))
            AppSettings.chooseDatabase(home + "/junk.sqlite")
            console.warn("walk: 28-junk refusal=" + AppSettings.databaseRefusal + " relaunch=" + AppSettings.needsRelaunch) } },
        { name: "29-new", enter: function () { const home = String(StandardPaths.writableLocation(StandardPaths.HomeLocation))
            AppSettings.chooseDatabase(home + "/new.sqlite")
            console.warn("walk: 29-new refusal=" + AppSettings.databaseRefusal + " relaunch=" + AppSettings.needsRelaunch
                + " stored=" + AppSettings.databasePath.endsWith("/new.sqlite") + " note=" + AppSettings.databaseNote)
            settingsWindow.relaunchNow() } },
        { name: "30-default", enter: function () { AppSettings.useDefaultLocation()
            console.warn("walk: 30-default stored=" + AppSettings.databasePath + " note=" + AppSettings.databaseNote); settingsWindow.showLogs() } },
        { name: "31-back", enter: function () { settingsWindow.close(); root.requestActivate() } },
    ]
    // The `grid` walk (B8b2): the Antiope grid, drawn by CellRow (`painted`) and by one Text per run (`text`), exact pixels then frame times.
    property bool walkPaused: false              // a step that measures asynchronously holds the walk
    readonly property var gridSteps: [
        { name: "g01-pattern", enter: function () { gridLoader.active = true; gridLoader.item.variant = Recette.gridVariant || "painted"; gridLoader.item.pattern() } },
        { name: "g02-scroll", enter: function () { root.walkPaused = true; const probe = gridLoader.item
            probe.measure(1, 600, function (s1) { console.warn("grid: " + probe.variant + " size=" + probe.size + " step=1 " + s1)
            probe.measure(10, 600, function (s10) { console.warn("grid: " + probe.variant + " size=" + probe.size + " step=10 " + s10)
            if (Recette.gridVariant === "" && probe.variant === "painted") { probe.variant = "text"; probe.measure(1, 600, function (t1) {
                console.warn("grid: text size=" + probe.size + " step=1 " + t1); probe.measure(10, 600, function (t10) {
                console.warn("grid: text size=" + probe.size + " step=10 " + t10); root.walkPaused = false }) }) } else root.walkPaused = false }) }) } },
    ]
    readonly property var steps: Recette.walk === "grid" ? gridSteps : Recette.walk === "library" ? librarySteps : Recette.walk === "chart" ? chartSteps : Recette.walk === "chart-live" ? chartLiveSteps : Recette.walk === "journal" ? journalSteps : Recette.walk === "garden" ? gardenSteps : Recette.walk === "mark-all" ? markAllSteps : walkSteps
    property bool pressedDefault: false          // the test hook (Recette.pressDefault) pressed "Use Default Database"
    property int walkIndex: -2                   // -2 not started, -1 launch screen, 0… steps
    function settled() { return Launcher.phase === Launcher.Ready || Launcher.phase === Launcher.TooNew || Launcher.phase === Launcher.Failed }
    function slug() { return Launcher.phase === Launcher.Ready ? "ready" : Launcher.phase === Launcher.TooNew ? "too-new" : "failed" }
    function startWalkIfSettled() { if (Recette.active && walkIndex === -2 && settled()) { walkIndex = -1; walkTimer.start() } }
    Connections { target: Launcher; function onPhaseChanged() { root.startWalkIfSettled() } }
    Timer {
        id: walkTimer; interval: 400
        onTriggered: {
            if (root.walkPaused) { walkTimer.start(); return }
            const name = root.walkIndex < 0 ? (root.pressedDefault ? "02-" : "01-") + root.slug() : root.steps[root.walkIndex].name
            if (!Recette.grab(name)) { Qt.exit(1); return }
            if (root.walkIndex === -1 && !root.pressedDefault && Recette.pressDefault !== "" && root.settled() && Launcher.phase !== Launcher.Ready) {
                root.pressedDefault = true; root.walkIndex = -2
                Recette.prepareDefault(); root.useDefaultDatabase()
                return
            }
            if (Reader.isExtracting || Runs.longRunActive) { walkTimer.start(); return }   // a fetch or a build in flight: grab again once it ends, and never quit during one
            root.walkIndex += 1
            if (Launcher.phase !== Launcher.Ready || root.walkIndex >= root.steps.length) { Qt.exit(0); return }
            root.steps[root.walkIndex].enter()
            walkTimer.start()
        }
    }
    Timer { interval: 60000; running: Recette.active; onTriggered: { console.error("ressac-kde: the screenshot walk timed out"); Qt.exit(1) } }

    Component.onCompleted: { Launcher.launch(); root.startWalkIfSettled() }

    menuBar: MainMenu { id: mainMenu; page: SourcesModel.emptyLibrary ? null : readingLoader.item }

    pageStack.initialPage: Kirigami.Page {
        globalToolBarStyle: Kirigami.ApplicationHeaderStyle.None
        padding: 0

        // Files dropped on the window; under everything, so the folder drop targets above it win.
        DropArea {
            id: fileDrop
            anchors.fill: parent
            keys: ["text/uri-list"]
            enabled: Launcher.phase === Launcher.Ready
            onDropped: (drop) => { if (!Library.busy && Library.openUrls(drop.urls.map(String))) drop.accept(Qt.CopyAction) }
        }

        Loader {
            id: readingLoader
            anchors.fill: parent
            visible: !SourcesModel.emptyLibrary
            active: Launcher.phase === Launcher.Ready
            source: Qt.resolvedUrl("ReadingPage.qml")
        }

        // The `grid` walk's probe, over the page; parented to the window overlay so its origin is the window's (the test counts pixels from it, below the menu bar).
        Loader { id: gridLoader; parent: QQC2.Overlay.overlay; anchors.fill: parent; z: 100; active: false; source: "GridProbe.qml" }

        FirstRun {
            anchors.fill: parent
            visible: Launcher.phase === Launcher.Ready && SourcesModel.emptyLibrary
            onChooseRequested: root.importOpml()
        }

        // The drop target's dashed accent outline.
        Rectangle {
            anchors.fill: parent
            anchors.margins: 6
            visible: fileDrop.containsDrag
            enabled: false
            radius: 10
            color: Qt.alpha(Kirigami.Theme.highlightColor, 0.05)
            Shape {
                id: dashes
                anchors.fill: parent
                ShapePath {
                    strokeColor: Kirigami.Theme.highlightColor
                    strokeWidth: 3
                    strokeStyle: ShapePath.DashLine
                    dashPattern: [8 / 3, 6 / 3]
                    fillColor: "transparent"
                    PathRectangle { x: 1.5; y: 1.5; width: dashes.width - 3; height: dashes.height - 3; radius: 10 }
                }
            }
        }

        ColumnLayout {
            anchors.centerIn: parent
            width: Math.min(parent.width - Kirigami.Units.gridUnit * 4, Kirigami.Units.gridUnit * 30)
            spacing: Kirigami.Units.largeSpacing

            QQC2.BusyIndicator {
                visible: Launcher.phase === Launcher.Starting || Launcher.phase === Launcher.Migrating
                running: true
                Layout.alignment: Qt.AlignHCenter
            }

            QQC2.Label {
                visible: Launcher.phase === Launcher.Migrating
                text: Launcher.message
                Layout.alignment: Qt.AlignHCenter
            }

            Kirigami.Heading {
                visible: Launcher.phase === Launcher.TooNew
                level: 3
                text: Launcher.message
                wrapMode: Text.Wrap
                horizontalAlignment: Text.AlignHCenter
                Layout.fillWidth: true
            }

            QQC2.Label {
                visible: Launcher.phase === Launcher.Failed
                text: Launcher.message
                wrapMode: Text.Wrap
                horizontalAlignment: Text.AlignHCenter
                Layout.fillWidth: true
            }

            RowLayout {
                visible: Launcher.phase === Launcher.TooNew || Launcher.phase === Launcher.Failed
                Layout.alignment: Qt.AlignHCenter
                spacing: Kirigami.Units.smallSpacing

                QQC2.Button {
                    text: "Use Default Database"
                    onClicked: root.useDefaultDatabase()
                }

                QQC2.Button {
                    visible: Launcher.phase === Launcher.Failed
                    text: "Quit Ressac"
                    onClicked: Qt.quit()
                }
            }
        }
    }
}
