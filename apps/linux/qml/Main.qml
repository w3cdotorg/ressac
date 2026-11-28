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
    // Antiope needs its minimum of cells (spec §7.9); Native its three panes.
    readonly property bool antiope: readingLoader.item !== null && readingLoader.item.antiope
    minimumWidth: antiope ? Math.ceil(Antiope.minimumCells * 0.6 * AppSettings.readerSizeAntiope) : 960
    minimumHeight: 500
    visible: true

    // The mosaic scheme's object, made with the window: the scheme answers through it, and without
    // it every image would get the failure placeholder and no `failed` (QML singletons are made on
    // first use).
    readonly property QtObject mosaics: Mosaics

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
    Binding { target: mainMenu.seaChart; property: "enabled"; value: Launcher.phase === Launcher.Ready }
    Connections { target: mainMenu.seaChart; function onTriggered() { root.openChart() } }
    Connections {
        target: Chart
        function onOpenFeed(key) {
            SourcesModel.selectKey(key)
            // Dead on Wayland (no Window.Minimized there: observed in the recette, not re-verified; the raise below is what brings it back); harmless on X11.
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
            Recette.key(Qt.Key_S, "s", 0); const s = EntriesModel.selectedStarred; const toolbar1 = page.starButton.text
            Recette.key(Qt.Key_S, "s", 0); const s2 = EntriesModel.selectedStarred
            console.warn("walk: 22-reader-keys focus=" + focus + " k=" + k + " j=" + j + " m=" + m + " m2=" + m2 + " s=" + s + " s2=" + s2
                + " toolbar=" + toolbar1 + "," + page.starButton.text) } },
        { name: "23-menu", enter: function () { mainMenu.openMenu("article")
            console.warn("walk: 23-menu read=" + mainMenu.toggleRead.text + " enabled=" + mainMenu.toggleRead.enabled + " star=" + mainMenu.toggleStar.text
                + " toolbar=" + readingLoader.item.starButton.text) } },
        { name: "24-text-size", enter: function () { mainMenu.closeMenus(); const before = AppSettings.readerSize
            mainMenu.biggerText.trigger(); const bigger = AppSettings.readerSize
            mainMenu.actualSize.trigger(); console.warn("walk: 24-text-size bigger=" + bigger + " actual=" + AppSettings.readerSize)
            AppSettings.readerSize = before } },         // the walk leaves the setting as it found it
        // The fixture: edition 1 built 2026-10-10T08:00Z with 1 unreachable feed; This Week has entries 5 and 6
        // (Daring Fireball), The Wire entry 1 (xkcd.com) then "+2 more in xkcd.com".
        { name: "23-journal", enter: function () { SourcesModel.selectKey("journal")
            console.warn("walk: 23-journal rows=" + EntriesModel.rowCount() + " phase=" + Journal.phase + " placeholder=" + Journal.placeholder
                + " header=" + Journal.header + " focus=" + readingLoader.item.focusName()) } },
        // The closing states (spec §8.2), so that `--screenshot <dir>` alone shows every page and window. Needs Triage
        // with its inspector is `06-garden` above, and is not shown twice.
        { name: "91-antiope-101", enter: function () { root.showAntiope("91-antiope-101") } },
        { name: "92-antiope-100", enter: function () { root.antiopeIndex("92-antiope-100") } },
        { name: "93-antiope-899", enter: function () { root.antiopeGuide("93-antiope-899") } },
        { name: "94-antiope-104", enter: function () { root.antiopeDial("94-antiope-104", 104, "selected") } },
        { name: "95-antiope-800", enter: function () { root.antiopeDial("95-antiope-800", 800, "selected") } },
        { name: "96-chart", enter: function () { root.showChart("96-chart") } },
        { name: "97-settings", enter: function () { root.showSettings("97-settings") } },
        { name: "98-native", enter: function () { root.showNative("98-native") } }
    ]
    // The states the closing steps of the default walk and the `smoke` walk share; each logs `walk: <tag> …`.
    function showGarden(tag) {
        SourcesModel.selectKey("garden:needs-triage")
        root.waitFor(() => Garden.title !== "", 5000, function () {
            console.warn("walk: " + tag + " garden=" + SourcesModel.isGarden + " inspector=" + Garden.title) })
    }
    function showAntiope(tag) {
        SourcesModel.selectKey("unread"); AppSettings.appearance = "antiope"
        root.waitFor(() => readingLoader.item.look === "antiope" && readingLoader.item.antiopeView !== null, 5000, function () {
            console.warn("walk: " + tag + " look=" + readingLoader.item.look + " page=" + Antiope.currentPage) })
    }
    function antiopeIndex(tag) {
        Recette.key(Qt.Key_S, "s", Qt.AltModifier)
        console.warn("walk: " + tag + " reader=" + Antiope.readerPage + " page=" + Antiope.currentPage)
    }
    function antiopeGuide(tag) {
        Recette.key(Qt.Key_Question, "?", Qt.ShiftModifier)
        console.warn("walk: " + tag + " reader=" + Antiope.readerPage + " page=" + Antiope.currentPage)
    }
    function antiopeDial(tag, page, lead) {
        root.dial(page)
        const key = "selected=" + SourcesModel.selectedKey; const number = "page=" + Antiope.currentPage
        console.warn("walk: " + tag + " " + (lead === "selected" ? key + " " + number : number + " " + key))
    }
    // The Sea Chart drawn: the page's own probe sees more than eight colours on its canvas (asked at most every 250 ms).
    property var chartProbe: ({ asking: false, asked: 0, colours: 0 })
    function chartDrawn() {
        const p = root.chartProbe
        if (p.colours <= 8 && !p.asking && Date.now() - p.asked >= 250 && chartLoader.item !== null) {
            p.asking = true; p.asked = Date.now()
            chartLoader.item.probe("", function (result) { p.asking = false; p.colours = Number(/colours=(\d+)/.exec(result)[1]) })
        }
        return p.colours > 8
    }
    function showChart(tag) {
        root.chartProbe = { asking: false, asked: 0, colours: 0 }
        root.openChart()
        root.waitFor(root.chartDrawn, 15000, function () {
            console.warn("walk: " + tag + " chart=" + (root.chartProbe.colours > 8 ? "drawn" : "blank") + " colours=" + root.chartProbe.colours) })
    }
    function showSettings(tag) {
        if (chartLoader.item !== null) chartLoader.item.close()
        root.openSettings(); settingsWindow.showPage("reading")
        console.warn("walk: " + tag + " appearance=" + AppSettings.appearance)
    }
    function showNative(tag) {
        settingsWindow.close(); root.requestActivate(); AppSettings.appearance = "native"
        root.waitFor(() => readingLoader.item.look === "native", 5000, function () {
            console.warn("walk: " + tag + " look=" + readingLoader.item.look) })
    }
    // The `smoke` walk (B10a, spec §8.3): every page and window once, keys reaching the database. Run on a copy of
    // the fixture whose feeds sit at 127.0.0.<id>:9; the log lines carry the ids the test checks.
    readonly property var smokeSteps: [
        { name: "s01-native", enter: function () { AppSettings.appearance = "native"; SourcesModel.selectKey("unread"); EntriesModel.select(0)
            console.warn("walk: smoke-s01-native entries=" + EntriesModel.rowCount() + " page=" + Reader.pageUrl) } },
        // j leaves the first entry (read), m reads the second, s stars it; the line names the rows' own flags.
        { name: "s02-keys", enter: function () {
            const first = EntriesModel.selectedEntryId
            Recette.key(Qt.Key_J, "j", 0); const second = EntriesModel.selectedEntryId
            Recette.key(Qt.Key_M, "m", 0); Recette.key(Qt.Key_S, "s", 0)
            root.waitFor(() => EntriesModel.selectedRead && EntriesModel.selectedStarred, 5000, function () {
                const read = [first, second].filter(id => root.rowFlag(id, 5)).join(",")
                console.warn("walk: smoke-s02-keys selected=" + EntriesModel.selectedEntryId + " read=" + read
                    + " starred=" + [second].filter(id => root.rowFlag(id, 6)).join(",")) }) } },
        { name: "s03-journal", enter: function () { SourcesModel.selectKey("journal")
            console.warn("walk: smoke-s03-journal journal=" + (SourcesModel.selectedKey === "journal")) } },
        { name: "s04-garden", enter: function () { root.showGarden("smoke-s04-garden") } },
        { name: "s05-antiope", enter: function () { root.showAntiope("smoke-s05-antiope") } },
        { name: "s06-p201", enter: function () { root.antiopeDial("smoke-s06-p201", 201, "page") } },
        { name: "s07-p100", enter: function () { root.antiopeIndex("smoke-s07-p100") } },
        { name: "s08-p899", enter: function () { root.antiopeGuide("smoke-s08-p899") } },
        { name: "s09-p104", enter: function () { root.antiopeDial("smoke-s09-p104", 104, "selected") } },
        { name: "s10-p800", enter: function () { root.antiopeDial("smoke-s10-p800", 800, "selected") } },
        { name: "s11-chart", enter: function () { root.showChart("smoke-s11-chart") } },
        { name: "s12-settings", enter: function () { root.showSettings("smoke-s12-settings") } },
        { name: "s13-native-again", enter: function () { root.showNative("smoke-s13-native-again") } }
    ]
    // One flag of an entry's row (role 5 read, 6 starred), false when the row is not in the list.
    function rowFlag(id, role) {
        for (let row = 0; row < EntriesModel.rowCount(); row++) {
            const index = EntriesModel.index(row, 0)
            if (EntriesModel.data(index, 256) === id) return EntriesModel.data(index, 256 + role) === true
        }
        return false
    }
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
    // The `redraw` walk (recette fix 1): frames alone never redraw a web view; a stop of the window's scene graph
    // (what minimising does) is followed by one redraw at the next frames, the view 1 px shorter for one of them.
    // The walk emits the window's own sceneGraphAboutToStop, the very signal Qt emits when the window is no longer
    // exposed, and WebRedraw listens to nothing else. Its other listener, WebEngine's releaseResources, acts only
    // under the native (GPU) compositor; offscreen, the views use the software one, so nothing else changes.
    property var readerHeights: []                // the view's height before, during and after the redraw
    property var chartHeights: []
    property int chartRedrawsBefore: 0            // the chart window may stop once while it opens
    function watchRedraw(owner, heights) {
        heights.push(owner.viewHeight)
        // Once every binding of the new inset has run: the anchors moved the view's bottom edge.
        owner.redraw.insetChanged.connect(function () { Qt.callLater(function () { heights.push(owner.viewHeight) }) })
    }
    readonly property var redrawSteps: [
        { name: "02-select", enter: function () { SourcesModel.selectKey("unread"); EntriesModel.select(0) } },
        { name: "03-frames", enter: function () { frameDriver.visible = true } },
        { name: "04-stop", enter: function () { const pane = readingLoader.item.reader, r = pane.redraw
            const idle = "redraws=" + r.redraws + " inset=" + r.inset + " stopped=" + r.stopped
            root.watchRedraw(pane, root.readerHeights)
            root.sceneGraphAboutToStop()
            console.warn("walk: 04-stop idle " + idle + " then redraws=" + r.redraws + " stopped=" + r.stopped) } },
        { name: "05-wait", enter: function () {} },
        { name: "06-redrawn", enter: function () { const r = readingLoader.item.reader.redraw; frameDriver.visible = false
            console.warn("walk: 06-redrawn redraws=" + r.redraws + " inset=" + r.inset + " stopped=" + r.stopped
                + " heights=" + root.readerHeights.join(">") + " page=" + Reader.pageUrl) } },
        { name: "07-chart", enter: function () { root.openChart() } },
        { name: "08-wait", enter: function () {} },
        { name: "09-chart-stop", enter: function () { const w = chartLoader.item, r = w.redraw
            const idle = "inset=" + r.inset + " stopped=" + r.stopped
            root.chartRedrawsBefore = r.redraws
            root.watchRedraw(w, root.chartHeights)
            w.sceneGraphAboutToStop(); w.requestUpdate()
            console.warn("walk: 09-chart-stop idle " + idle + " then stopped=" + r.stopped + " onScreen=" + Live.chartOnScreen) } },
        { name: "10-wait", enter: function () { chartLoader.item.requestUpdate() } },
        { name: "11-wait", enter: function () { chartLoader.item.requestUpdate() } },
        { name: "12-chart-redrawn", enter: function () { const r = chartLoader.item.redraw
            console.warn("walk: 12-chart-redrawn added=" + (r.redraws - root.chartRedrawsBefore) + " inset=" + r.inset + " stopped=" + r.stopped
                + " onScreen=" + Live.chartOnScreen + " heights=" + root.chartHeights.join(">")) } }
    ]
    // The `settings-size` walk (recette fix 2): the Text size row stays where it is whatever the size, Native and Antiope.
    function sizeRow(tag) {
        const s = settingsWindow
        const x = s.sizeSlider.visible ? s.sizeSlider : s.antiopeSlider
        console.warn("walk: " + tag + " size=" + Math.round(s.antiope ? AppSettings.readerSizeAntiope : AppSettings.readerSize)
            + " sliderX=" + Math.round(x.mapToItem(null, 0, 0).x) + " sliderW=" + Math.round(x.width)
            + " windowActive=" + Live.windowActive + " previewW=" + Math.round(s.previewLabel.width) + " ticks=" + x.Kirigami.StyleHints.tickMarkStepSize
            + " truncated=" + s.previewLabel.truncated + " lines=" + s.previewLabel.lineCount
            + " right=" + Math.round(s.previewLabel.mapToItem(null, 0, 0).x + s.previewLabel.width) + " winW=" + Math.round(s.width))
    }
    readonly property var settingsSizeSteps: [
        { name: "02-open", enter: function () { AppSettings.appearance = "native"; root.openSettings(); settingsWindow.showPage("reading"); settingsWindow.width = settingsWindow.minimumWidth
            // Scale 2 doubles Kirigami's grid unit: its widths, as they come out at the VM.
            if (Recette.walk === "settings-size-x2") { settingsWindow.previewWidth *= 2; settingsWindow.sliderWidth *= 2 } AppSettings.readerSize = 12 } },
        { name: "03-n12", enter: function () { root.sizeRow("03-n12") } },
        { name: "04-set20", enter: function () { AppSettings.readerSize = 20 } },
        { name: "05-n20", enter: function () { root.sizeRow("05-n20") } },
        { name: "06-set28", enter: function () { AppSettings.readerSize = 28 } },
        { name: "07-n28", enter: function () { root.sizeRow("07-n28") } },
        { name: "08-antiope", enter: function () { AppSettings.appearance = "antiope"; AppSettings.readerSizeAntiope = 15 } },
        { name: "09-a15", enter: function () { root.sizeRow("09-a15") } },
        { name: "10-set30", enter: function () { AppSettings.readerSizeAntiope = 30 } },
        { name: "11-a30", enter: function () { root.sizeRow("11-a30"); AppSettings.appearance = "native" } },
        // Settings in front, the main window not the active one (offscreen keeps both active, so the main window is minimised):
        // the list keeps polling, `Live.windowActive` follows the application.
        { name: "12-main-away", enter: function () { root.showMinimized() } },
        { name: "12b-wait", enter: function () {} },
        { name: "13-away", enter: function () { console.warn("walk: 13-away mainState=" + root.visibility + " mainActive=" + root.active + " settings=" + settingsWindow.visible
            + " app=" + (Qt.application.state === Qt.ApplicationActive) + " windowActive=" + Live.windowActive) } }
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
            console.warn("walk: 02-open open=" + Chart.open + " reduce=" + Chart.reduceMotion) } },
        // The chart is on screen once its window has drawn (its scene graph may stop once while it opens).
        { name: "03-wait", enter: function () { console.warn("walk: 03-wait onScreen=" + Live.chartOnScreen) } }, { name: "04-wait", enter: function () {} }, { name: "05-wait", enter: function () {} },
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
    // The `chart-edit` walk: an edit made in the Sea Chart's page, undone and redone with Ctrl+Z and Ctrl+Shift+Z sent to the
    // chart window by its title (`Recette.keyTo`), and a refused edit. The page's `Chart.update` and `Chart.edited` are wrapped
    // (window.__w, on a stand-in for the frozen window.Chart) to count what reaches it. A step that waits on the page holds the walk (`walkPaused`) and polls.
    Timer { id: chartTimer; property var next; onTriggered: next() }
    function chartLater(ms, fn) { chartTimer.interval = ms; chartTimer.next = fn; chartTimer.restart() }
    // Runs `script` in the chart page, then polls `until` (an expression of the page) every 250 ms, for 20 s at most; `then(result)`.
    function chartDo(script, until, then) {
        root.walkPaused = true
        const win = chartLoader.item
        const check = "(function () { try { return " + until + " } catch (e) { return null } })()"
        let tries = 0
        const poll = function () {
            win.runPage(check, function (result) {
                if (result || ++tries >= 80) { root.walkPaused = false; then(result) }
                else root.chartLater(250, poll)
            })
        }
        win.runPage(script, function () { poll() })
    }
    // The page's counters once the writes have settled (2 s: a second Chart.update would show).
    function chartFacts(then) {
        root.walkPaused = true
        root.chartLater(2000, function () {
            chartLoader.item.runPage("(function () { return 'seas=' + document.querySelectorAll('.log-sea').length"
                + " + ' undo=' + document.querySelector('.log-undo').textContent + ' redo=' + document.querySelector('.log-redo').textContent"
                + " + ' replies=' + __w.replies + ' updates=' + __w.updates })()", function (facts) { root.walkPaused = false; then(facts) })
        })
    }
    readonly property string chartSurveyButton: "[...document.querySelectorAll('.bar button')].find(b => b.textContent === '✒ Survey')"
    readonly property var chartEditSteps: [
        { name: "02-open", enter: function () { root.openChart() } },
        { name: "03-ready", enter: function () { root.chartDo("", "!!" + root.chartSurveyButton + " && !" + root.chartSurveyButton + ".hidden && typeof Chart === 'object'", function (ready) {
            root.chartDo("window.__w = {replies: 0, updates: 0, last: null};"
                + "const real = Chart; window.Chart = {load: (a, b) => real.load(a, b), undo: () => real.undo(), redo: () => real.redo(),"
                + " edited: text => { __w.replies++; __w.last = text; return real.edited(text) }, update: json => { __w.updates++; return real.update(json) }};", "typeof __w === 'object'", function () {
                console.warn("walk: 03-ready ready=" + !!ready + " title=" + Chart.windowTitle) }) }) } },
        { name: "10-survey", enter: function () { chartLoader.item.requestActivate()
            root.chartDo(root.chartSurveyButton + ".click()", "!!document.querySelector('.log-new-sea-button')", function (log) {
                console.warn("walk: 10-survey log=" + !!log) }) } },
        { name: "18-new-sea", enter: function () {
            root.chartDo("document.querySelector('.log-new-sea-button').click();"
                + "const input = document.querySelector('.log-new-sea .name-form-input'); input.value = 'Sea of Tests';"
                + "input.dispatchEvent(new Event('input', {bubbles: true})); input.form.requestSubmit();",
                "__w.replies >= 1 && __w.updates >= 1 && document.querySelectorAll('.log-sea').length >= 1", function (done) {
                root.chartFacts(function (facts) { console.warn("walk: 20-made " + facts.replace(/ redo=.*? (replies=)/, " $1") + " chart=" + (done ? "ready" : "timeout")) }) }) } },
        { name: "22-undo", enter: function () { chartLoader.item.requestActivate()
            const sent = Recette.keyTo(Chart.windowTitle, Qt.Key_Z, "z", Qt.ControlModifier)
            root.chartDo("", "__w.updates >= 2 && document.querySelectorAll('.log-sea').length === 0", function () {
                root.chartFacts(function (facts) { console.warn("walk: 24-undone sent=" + sent + " " + facts.replace(/^(seas=\d+) undo=.*? (redo=)/, "$1 $2")) }) }) } },
        { name: "26-redo", enter: function () {
            const sent = Recette.keyTo(Chart.windowTitle, Qt.Key_Z, "Z", Qt.ControlModifier | Qt.ShiftModifier)
            root.chartDo("", "__w.updates >= 3 && document.querySelectorAll('.log-sea').length === 1", function () {
                root.chartFacts(function (facts) { console.warn("walk: 28-redone sent=" + sent + " " + facts.replace(/ redo=.*? (replies=)/, " $1")) }) }) } },
        { name: "29-nowhere", enter: function () { console.warn("walk: 29-nowhere sent=" + Recette.keyTo("No such window", Qt.Key_Z, "z", Qt.ControlModifier)) } },
        { name: "30-refused", enter: function () {
            // The page's own check would stop a clash it can see; this one goes to the core by the bridge, as a stale page's would.
            root.chartDo("window.webkit.messageHandlers.ressac.postMessage({type: 'edit', seq: 9999, edit: JSON.stringify({op: 'new_sea', name: 'sea of tests'})});",
                "__w.replies >= 4", function (replied) {
                root.chartFacts(function (facts) { chartLoader.item.runPage("(function () { const r = JSON.parse(__w.last); return 'ok=' + r.ok + ' refused=' + r.refused + ' error=' + r.error })()",
                    function (reply) { console.warn("walk: 30-refused " + facts.replace(/^(seas=\d+) .*? (replies=\d+ updates=\d+)$/, "$1 $2") + " " + reply + " answered=" + !!replied) }) }) }) } },
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
    // The `antiope` walk (B8b6): the look switched live, pages dialled, columns, the Minitel keys, back to Native.
    readonly property var antiopeSteps: [
        { name: "a01-antiope", enter: function () { AppSettings.appearance = "antiope" } },
        { name: "a02-dial", enter: function () { const p = readingLoader.item
            console.warn("walk: a01-antiope look=" + p.look + " page=" + Antiope.currentPage + " column=" + Antiope.column + " focus=" + p.focusName())
            const o = p.antiopeView.mapToItem(null, 0, 0)
            console.warn("walk: a01-origin x=" + o.x + " y=" + o.y + " width=" + p.antiopeView.width + " minimum=" + root.minimumWidth)
            Recette.key(Qt.Key_2, "2", 0); Recette.key(Qt.Key_0, "0", 0); Recette.key(Qt.Key_1, "1", 0)
            console.warn("walk: a02-dial selected=" + SourcesModel.selectedKey + " page=" + Antiope.currentPage) } },
        { name: "a03-not-found", enter: function () { Recette.key(Qt.Key_4, "4", 0); Recette.key(Qt.Key_3, "3", 0); Recette.key(Qt.Key_7, "7", 0)
            console.warn("walk: a03-not-found selected=" + SourcesModel.selectedKey + " page=" + Antiope.currentPage) } },
        { name: "a04-columns", enter: function () {
            // The next digit clears NOT FOUND at once; Esc (ANNULATION) then drops the half-typed page.
            const shown = Antiope.header.indexOf("NOT FOUND") >= 0; Recette.key(Qt.Key_1, "1", 0); const cleared = Antiope.header.indexOf("NOT FOUND") < 0
            Recette.key(Qt.Key_Escape, "", 0)
            console.warn("walk: a03-cleared shown=" + shown + " cleared=" + cleared + " page=" + Antiope.currentPage)
            Recette.key(Qt.Key_Right, "", 0)
            console.warn("walk: a04-entries column=" + Antiope.column + " selected=" + EntriesModel.selectedEntryId); Recette.key(Qt.Key_Right, "", 0) } },
        { name: "a05-sommaire", enter: function () { console.warn("walk: a04-reader column=" + Antiope.column + " focus=" + readingLoader.item.focusName())
            Recette.key(Qt.Key_S, "s", Qt.AltModifier); console.warn("walk: a05-sommaire reader=" + Antiope.readerPage + " page=" + Antiope.currentPage) } },
        { name: "a06-guide", enter: function () { Recette.key(Qt.Key_Question, "?", Qt.ShiftModifier)
            console.warn("walk: a06-guide reader=" + Antiope.readerPage + " page=" + Antiope.currentPage) } },
        { name: "a07-suite", enter: function () { const before = EntriesModel.selectedEntryId; Recette.key(Qt.Key_J, "j", 0)
            console.warn("walk: a07-suite reader=" + Antiope.readerPage + " same=" + (EntriesModel.selectedEntryId === before)) } },
        { name: "a08-index-arrows", enter: function () { Recette.key(Qt.Key_Left, "", 0); Recette.key(Qt.Key_Left, "", 0); Recette.key(Qt.Key_Down, "", 0)
            console.warn("walk: a08-index-arrows column=" + Antiope.column + " page=" + Antiope.currentPage) } },
        { name: "a09-native", enter: function () {
            // A search typed and an entry selected: the switch keeps both, and Native's letters work at once.
            EntriesModel.searchText = "comic"
            const key = SourcesModel.selectedKey; const entry = EntriesModel.selectedEntryId
            AppSettings.appearance = "native"
            console.warn("walk: a09-native look=" + readingLoader.item.look + " selected=" + SourcesModel.selectedKey + " focus=" + readingLoader.item.focusName())
            console.warn("walk: a09-kept source=" + (SourcesModel.selectedKey === key) + " entry=" + (EntriesModel.selectedEntryId === entry)
                + " search=" + EntriesModel.searchText + " view=" + (readingLoader.item.antiopeView === null))
            SourcesModel.selectKey("feed:1"); const before = EntriesModel.selectedEntryId     // potes has no entries
            Recette.key(Qt.Key_J, "j", 0)
            console.warn("walk: a09-letters verb=" + readingLoader.item.lastVerb + " handled=" + readingLoader.item.lastHandled
                + " moved=" + (EntriesModel.selectedEntryId !== before) + " focus=" + readingLoader.item.focusName())
            EntriesModel.searchText = "" } },
        { name: "a10-awkward", enter: function () { AppSettings.appearance = "antiope"; SourcesModel.selectKey("feed:1")
            console.warn("walk: a10-awkward focus=" + readingLoader.item.focusName()) } },
    ]
    // The `antiope-pages` walk (B8c4): every Antiope page on screen. The test's copy: an edition of
    // entries 1 and 3 with an overflow row, feed 2 waiting for triage (P800), no network.
    // An encoded `Line` (pages::Line::encode): id, flags, action, cells; a run is fg, bg, text.
    function lineField(encoded, field) { return String(encoded).split("\u001d")[field] || "" }
    function lineText(encoded) { return lineField(encoded, 3).split("\u001f").map(run => run.slice(2)).join("") }
    function dial(page) { for (const d of String(page)) Recette.key(Qt.Key_0 + Number(d), d, 0) }
    function journalEnd() { return Antiope.journalRows.some(row => root.lineField(row, 0) === "end") }
    // The header's page label (its first run): `P800`, `P41` while digits are typed, `P437 NOT FOUND`.
    function headerPage() { return Antiope.header.split("\u001f")[0].slice(2).trim() }
    // An own write to land (`EntriesModel.wrote`), at most `ms`: the walk waits through `waitFor`.
    property bool waitWrote: false
    Connections { target: EntriesModel; enabled: root.waitWrote; function onWrote() { root.waitWrote = false } }
    function waitForWrote(ms) { root.waitWrote = true; root.waitFor(() => !root.waitWrote, ms) }
    // Holds the walk until `test()` holds (polled every 20 ms) or `ms` passed: a write or a
    // popup's transition lands in its own time on a loaded machine.
    // `done`, when given, runs once the wait is over (it may wait again).
    function waitFor(test, ms, done) { walkWait.test = test; walkWait.done = done || null; walkWait.until = Date.now() + ms; root.walkPaused = true; walkWait.start() }
    Timer {
        id: walkWait
        interval: 20
        repeat: true
        property var test: null
        property var done: null
        property real until: 0
        onTriggered: {
            const ok = test()
            if (!ok && Date.now() <= until) return
            stop(); root.walkPaused = false
            if (!ok) console.warn("walk: wait timed out")      // the step that needed it fails at its own check, with this line above
            const d = done; done = null; if (d) d()
        }
    }
    // Holds the walk while the reader's page answers `script` (app world, every 50 ms) with a result
    // that is not `ready` yet, at most `ms`; then `done(result)` (the last answer, undefined if none).
    function waitForPage(reader, script, ready, ms, done) {
        pagePoll.reader = reader; pagePoll.script = script; pagePoll.ready = ready; pagePoll.done = done
        pagePoll.until = Date.now() + ms; pagePoll.last = undefined; pagePoll.asking = false
        root.walkPaused = true; pagePoll.start()
    }
    Timer {
        id: pagePoll
        interval: 50
        repeat: true
        property var reader: null
        property string script: ""
        property var ready: null
        property var done: null
        property var last
        property real until: 0
        property bool asking: false
        function finish() { stop(); asking = false; const d = done; done = null; root.walkPaused = false; if (d) d(last) }
        onTriggered: {
            if (Date.now() > until) { finish(); return }
            if (asking) return
            asking = true
            reader.runAppScript(script, function (result) {
                if (!pagePoll.running) return
                pagePoll.asking = false; pagePoll.last = result
                if (pagePoll.ready(result)) pagePoll.finish()
            })
        }
    }
    readonly property var antiopePagesSteps: [
        { name: "c01-sommaire", enter: function () { AppSettings.appearance = "antiope"
            const o = readingLoader.item.antiopeView.mapToItem(null, 0, 0)
            console.warn("walk: c01-origin x=" + o.x + " y=" + o.y)
            Recette.key(Qt.Key_S, "s", Qt.AltModifier)
            console.warn("walk: c01-sommaire reader=" + Antiope.readerPage + " rows=" + Antiope.readerRows.length
                + " first=" + root.lineText(Antiope.readerRows[0]).trim()) } },
        { name: "c02-guide", enter: function () { Recette.key(Qt.Key_Question, "?", Qt.ShiftModifier)
            console.warn("walk: c02-guide reader=" + Antiope.readerPage) } },
        { name: "c03-journal", enter: function () { root.dial(104)
            console.warn("walk: c03-journal selected=" + SourcesModel.selectedKey + " rows=" + Antiope.journalRows.length + " end=" + root.journalEnd()) } },
        { name: "c04-suite-end", enter: function () {
            // The edition's last entry clicked (its row's action), then SUITE past it.
            const entries = Antiope.journalRows.map(row => root.lineField(row, 2)).filter(action => action.startsWith("entry:"))
            if (entries.length === 0) {                     // an empty edition (another copy): nothing to read
                console.warn("walk: c04-suite-end reader=" + Antiope.readerPage + " edition=empty")
                return
            }
            readingLoader.item.antiopeView.activate(entries[entries.length - 1], 0)
            Recette.key(Qt.Key_J, "j", 0)
            console.warn("walk: c04-suite-end reader=" + Antiope.readerPage + " read=" + EntriesModel.selectedRead) } },
        { name: "c05-retour", enter: function () { Recette.key(Qt.Key_K, "k", 0)
            console.warn("walk: c05-retour reader=" + Antiope.readerPage) } },
        { name: "c05b-end-jump", enter: function () { Recette.key(Qt.Key_J, "j", 0); const before = Antiope.readerPage
            root.dial(201)
            console.warn("walk: c05b-end-jump before=" + before + " reader=" + Antiope.readerPage + " page=" + Antiope.currentPage) } },
        { name: "c05c-change-away", enter: function () {
            // A database change away from P104: the edition's entries marked read from here.
            root.waitForWrote(5000)
            EntriesModel.markAllReadIn("journal")
            console.warn("walk: c05c-change-away page=" + Antiope.currentPage) } },
        { name: "c05d-journal-again", enter: function () { root.dial(104)
            const read = (root.lineText(Antiope.journalRows[0]).match(/(\d+\/\d+) READ/) || ["", "?"])[1]
            console.warn("walk: c05d-journal-again selected=" + SourcesModel.selectedKey + " read=" + read + " end=" + root.journalEnd()) } },
        { name: "c06-garden", enter: function () { root.dial(800); Recette.key(Qt.Key_Right, "", 0)
            const view = readingLoader.item.antiopeView
            console.warn("walk: c06-garden selected=" + SourcesModel.selectedKey + " column=" + Antiope.column + " inspector=" + Garden.antiopeInspector.length
                + " entries-focus=" + view.entriesItem.activeFocus) } },
        { name: "c06b-garden-key", enter: function () { const before = Garden.countLabel; Recette.key(Qt.Key_K, "k", 0)
            console.warn("walk: c06b-garden-key message=" + Garden.message)
            root.waitFor(() => Garden.countLabel !== before, 5000) } },        // the keep's write
        { name: "c06c-repairs", enter: function () { const count = Garden.countLabel
            Recette.key(Qt.Key_R, "R", Qt.ShiftModifier)
            console.warn("walk: c06c-repairs confirming=" + Garden.confirming + " count=" + count)
            root.waitFor(() => root.activeFocusItem !== null && root.activeFocusItem.text === Garden.applyConfirmLabel, 3000) } },
        { name: "c06d-repairs-switch", enter: function () {
            // The question's confirm button has the keyboard; a look switch drops the question.
            const focused = root.activeFocusItem; const focus = focused && focused.text !== undefined ? focused.text : String(focused)
            AppSettings.appearance = "native"
            console.warn("walk: c06d-repairs-switch focus=" + focus + " look=" + readingLoader.item.look + " confirming=" + Garden.confirming) } },
        { name: "c06e-back", enter: function () { AppSettings.appearance = "antiope"
            console.warn("walk: c06e-back look=" + readingLoader.item.look + " page=" + Antiope.currentPage + " confirming=" + Garden.confirming) } },
        { name: "c07-prompt-digits", enter: function () { mainMenu.search.trigger()
            // After each press: the search text, and the header's page label (never `P4`, `P41`, `P412`:
            // no page number is being typed, nor NOT FOUND).
            const typed = []; const headers = []
            for (const d of "412") { Recette.key(Qt.Key_0 + Number(d), d, 0); typed.push(EntriesModel.searchText); headers.push(root.headerPage()) }
            Recette.key(Qt.Key_Return, "\r", 0)
            console.warn("walk: c07-prompt-digits search=" + EntriesModel.searchText + " page=" + Antiope.currentPage
                + " prompt=" + readingLoader.item.antiopeView.promptOpen + " typed=" + typed.join("|") + " headers=" + headers.join("|")
                + " after=" + root.headerPage()) } },
        { name: "c08-switch-with-prompt", enter: function () { mainMenu.search.trigger(); Recette.key(Qt.Key_X, "x", 0)
            AppSettings.appearance = "native"
            // The prompt went with the view, its text kept; a letter is routed as a command at once in Native
            // (this line shows the routing, not an effect: `lastHandled` says whether a list took the key).
            const p = readingLoader.item; const switched = "look=" + p.look + " search=" + EntriesModel.searchText + " focus=" + p.focusName()
            p.lastVerb = ""; p.lastHandled = false
            Recette.key(Qt.Key_J, "j", 0)
            console.warn("walk: c08-switch-with-prompt " + switched + " letter=" + p.lastVerb + " handled=" + p.lastHandled
                + " search-after=" + EntriesModel.searchText) } },
        { name: "c09-banner", enter: function () { Runs.showBanner("The database is busy (is `ressac` running in a terminal?)", true)
            AppSettings.appearance = "antiope"
            console.warn("walk: c09-banner banner=" + Runs.bannerVisible) } },
        { name: "c10-settings", enter: function () { root.openSettings()
            console.warn("walk: c10-settings appearance=" + AppSettings.effectiveAppearance + " bedstead=" + AppSettings.bedsteadAvailable) } },
    ]
    // The `antiope-scroll` walk: `j` held on long unread articles with one page loaded (Native, then
    // Antiope), the entries column scrolled (frame times), then `j` held again with 2000 rows loaded.
    // Each hold starts on rows the last one left unread.
    readonly property var antiopeScrollSteps: [
        { name: "s00-native", enter: function () { AppSettings.appearance = "native"; SourcesModel.selectKey("feed:1") } },
        { name: "s01-hold-j-native", enter: function () { root.walkPaused = true; holdJ.begin("native hold-j-page", 0, 60) } },
        { name: "s02-folder", enter: function () { AppSettings.appearance = "antiope"; SourcesModel.selectKey("feed:1") } },
        { name: "s03-hold-j-page", enter: function () { root.walkPaused = true; holdJ.begin("antiope hold-j-page", 70, 60) } },
        { name: "s04-scroll", enter: function () { root.walkPaused = true; const view = readingLoader.item.antiopeView
            view.scrollProbe(1, 600, function (s1) { console.warn("frames: antiope step=1 " + s1)
            view.scrollProbe(10, 600, function (s10) { console.warn("frames: antiope step=10 " + s10); root.walkPaused = false }) }) } },
        { name: "s05-hold-j", enter: function () { root.walkPaused = true; holdJ.begin("antiope hold-j", 140, 60) } },
    ]
    // `antiope-bridges`: Task 7's wiring alone (the directory, digits to Antiope, the header).
    readonly property var bridgeSteps: [
        { name: "b00", enter: function () { AppSettings.appearance = "antiope" } },
        { name: "b01", enter: function () { const before = Antiope.currentPage; const d = Antiope.perform("pageDigit2"); Antiope.perform("pageDigit0")
            const s = Antiope.perform("pageDigit1"); readingLoader.item.antiopeView.apply(s); Antiope.tick()
            console.warn("walk: b01 current=" + before + " index=" + Antiope.indexRows.length + " digit=" + d + " select=" + s
                + " header=" + (Antiope.header.indexOf("P201") >= 0 ? "P201" : Antiope.header)) } },
    ]
    // The `mosaic` walk (B8d3): the test serves /photo.png (red), /broken.png (HTML) and /slow.png (30 s) on
    // loopback; entry 9001 shows the photo and the broken image, 9002 the slow one, 9003 a failing address
    // with a space and an é. Probes read the page in the app world; the page's own scripts stay off.
    function mosaicPage(script, ready, done) { root.waitForPage(readingLoader.item.antiopeView.reader, script, ready, 10000, done) }
    readonly property var mosaicSteps: [
        { name: "d01-mosaic", enter: function () { AppSettings.appearance = "antiope"
            const o = readingLoader.item.antiopeView.mapToItem(null, 0, 0)
            console.warn("walk: d01-origin x=" + o.x + " y=" + o.y)
            SourcesModel.selectKey("feed:1"); EntriesModel.selectEntry(9001)
            // Until the photo's mosaic is drawn and the broken image's box is marked.
            root.mosaicPage('(() => { const g = document.querySelector("#m0 img"); const f = document.getElementById("m1");'
                + ' return !!(g && g.complete && g.naturalWidth > 0 && f && f.dataset.failed !== undefined) })()',
                r => r === true, function (r) { console.warn("walk: d01 ready=" + r + " page=" + Reader.pageUrl) }) } },
        { name: "d02-failed", enter: function () { root.mosaicPage('(() => { const m1 = document.getElementById("m1");'
            + ' return "m0=" + document.querySelector("#m0 img").naturalWidth + " m1failed=" + (m1.dataset.failed !== undefined)'
            + ' + " alt=" + getComputedStyle(m1.querySelector(".mosaic-alt")).display })()',
            r => true, function (r) { console.warn("walk: d02 " + r) }) } },
        { name: "d03-click", enter: function () { readingLoader.item.antiopeView.reader.clickSelector("#m0")
            root.mosaicPage('(() => { const b = document.getElementById("m0"); const g = b.querySelector("img");'
                + ' return { ready: b.classList.contains("original") && g.complete && g.naturalWidth > 0,'
                + ' line: "src=" + g.src + " original=" + b.classList.contains("original") } })()',
                r => r && r.ready, function (r) { console.warn("walk: d03 " + (r ? r.line : "no answer")) }) } },
        { name: "d04-i", enter: function () { Recette.key(Qt.Key_I, "i", 0)
            root.mosaicPage('document.querySelectorAll(".mosaic.original").length', r => true,
                function (r) { console.warn("walk: d04 originals=" + r) }) } },
        { name: "d05-i-again", enter: function () { Recette.key(Qt.Key_I, "i", 0)
            root.mosaicPage('document.querySelectorAll(".mosaic.original").length', r => true,
                function (r) { console.warn("walk: d05 originals=" + r) }) } },
        // The slow image requested, then another entry at once: the page cancels the request.
        { name: "d06-slow-reload", enter: function () { EntriesModel.selectEntry(9002)
            root.waitFor(() => Mosaics.inFlight >= 1, 10000, function () { const pending = Mosaics.inFlight
                EntriesModel.selectEntry(9003); console.warn("walk: d06 pending=" + pending) }) } },
        { name: "d07-settled", enter: function () {
            root.mosaicPage('(() => { const b = document.getElementById("m0"); return b ? { failed: b.dataset.failed !== undefined, mosaic: b.dataset.mosaic } : null })()',
                r => r && r.failed && Mosaics.inFlight === 0,
                function (r) { console.warn("walk: d07 inFlight=" + Mosaics.inFlight + " failed=" + (r ? r.failed : false) + " mosaic=" + (r ? r.mosaic : ""))
                    // The walk lingers: the test's server sees the stopped download close well before the exit.
                    root.waitFor(() => false, 2500) }) } },
    ]
    // `j` held (a press every 33 ms, the key repeat's pace) on long articles: the frame times while a 2 px
    // square flips on every frame (so a frame is always due), and the time each press takes. Then how
    // long the sidebar badge and Antiope's index take to show the marks (the coalesced counts reload),
    // and what the writes asked for meanwhile (`wrote` = a full reload, `settled` = the counts only).
    Timer {
        id: holdJ
        interval: 33
        repeat: true
        property int left: 0
        property int presses: 0
        property real worst: 0
        property real total: 0
        property int slow: 0                     // presses whose own handling (≥ 14 ms) fills a 16.7 ms frame
        property int first: -1
        property bool counting: false
        property int wrote: 0
        property int settled: 0
        property int invalidated: 0
        property int badgeBefore: 0
        property string indexBefore: ""
        property real released: 0
        property string label: ""
        function badge() { return SourcesModel.data(SourcesModel.index(SourcesModel.rowOfKey("feed:1"), 0), 256 + 4) }   // role "badge"
        function begin(name, row, count) {
            EntriesModel.select(row)
            label = name; left = count; presses = 0; worst = 0; total = 0; slow = 0; first = EntriesModel.selectedRow
            wrote = 0; settled = 0; invalidated = 0; counting = true
            badgeBefore = badge(); indexBefore = Antiope.indexRows.join("|")
            Recette.frameLogStart(); frameDriver.visible = true; start()
        }
        onTriggered: {
            if (left > 0) {
                const t0 = Date.now(); Recette.key(Qt.Key_J, "j", 0); const ms = Date.now() - t0
                left -= 1; presses += 1; total += ms; worst = Math.max(worst, ms); if (ms >= 14) slow += 1
                return
            }
            stop(); frameDriver.visible = false
            console.warn("frames: " + label + " " + Recette.frameLogStop() + " rows=" + EntriesModel.rowCount() + " presses=" + presses + " moved=" + (EntriesModel.selectedRow - first)
                + " press-mean=" + (total / Math.max(1, presses)).toFixed(2) + " press-max=" + worst + " slow=" + slow)
            released = Date.now(); holdCounts.start()
        }
    }
    Connections { target: EntriesModel; enabled: holdJ.counting; function onWrote() { holdJ.wrote++ } function onSettled() { holdJ.settled++ } }
    Connections { target: Live; enabled: holdJ.counting; function onInvalidated() { holdJ.invalidated++ } }
    Timer {
        id: holdCounts
        interval: 20
        repeat: true
        onTriggered: {
            const after = Date.now() - holdJ.released
            const caught = holdJ.badgeBefore - holdJ.badge() >= holdJ.presses
                && (AppSettings.appearance !== "antiope" || Antiope.indexRows.join("|") !== holdJ.indexBefore)
            if (!caught && after < 3000)
                return
            stop(); holdJ.counting = false
            console.warn("walk: " + holdJ.label + " counts caught=" + caught + " after=" + after + " badge-before=" + holdJ.badgeBefore + " badge-after=" + holdJ.badge()
                + " wrote=" + holdJ.wrote + " settled=" + holdJ.settled + " invalidated=" + holdJ.invalidated)
            root.walkPaused = false
        }
    }
    Rectangle {
        id: frameDriver
        parent: QQC2.Overlay.overlay
        visible: false
        width: 2; height: 2; color: "black"
        Connections { target: root; enabled: frameDriver.visible; function onFrameSwapped() { frameDriver.x = frameDriver.x > 0 ? 0 : 2 } }
    }
    readonly property var steps: Recette.walk === "antiope" ? antiopeSteps : Recette.walk === "antiope-pages" ? antiopePagesSteps : Recette.walk === "antiope-scroll" ? antiopeScrollSteps : Recette.walk === "antiope-bridges" ? bridgeSteps : Recette.walk === "mosaic" ? mosaicSteps : Recette.walk === "grid" ? gridSteps : Recette.walk === "library" ? librarySteps : Recette.walk === "chart" ? chartSteps : Recette.walk === "chart-live" ? chartLiveSteps : Recette.walk === "chart-edit" ? chartEditSteps : Recette.walk === "journal" ? journalSteps : Recette.walk === "garden" ? gardenSteps : Recette.walk === "mark-all" ? markAllSteps : Recette.walk === "redraw" ? redrawSteps : (Recette.walk === "settings-size" || Recette.walk === "settings-size-x2") ? settingsSizeSteps : Recette.walk === "smoke" ? smokeSteps : walkSteps
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
            if (root.walkIndex === -1) console.warn("walk: " + name + " seaChart=" + mainMenu.seaChart.enabled)
            if (root.walkIndex === -1 && !root.pressedDefault && Recette.pressDefault !== "" && root.settled() && Launcher.phase !== Launcher.Ready) {
                root.pressedDefault = true; root.walkIndex = -2
                Recette.prepareDefault(); root.useDefaultDatabase()
                return
            }
            if (Reader.isExtracting || Runs.longRunActive || (readingLoader.item !== null && readingLoader.item.proofPending)) { walkTimer.start(); return }   // a fetch, a build or the reader's proof page in flight: grab again once it ends, and never quit during one
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
        // Antiope's screen is black to the window's edges.
        background: Rectangle { color: root.antiope ? "black" : Kirigami.Theme.backgroundColor }

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
