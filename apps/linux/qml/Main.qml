import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.w3cdotorg.ressac

Kirigami.ApplicationWindow {
    id: root

    title: "Ressac"
    width: 1100
    height: 720
    minimumWidth: 900
    minimumHeight: 500
    visible: true

    function useDefaultDatabase() {
        AppSettings.databasePath = ""
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
                + " apply=" + Garden.applyTitle + " focus=" + readingLoader.item.focusName() + " sections=" + Garden.sectionTitles.join("|")) } },
        { name: "03-keep", enter: function () { Recette.key(Qt.Key_K, "k", 0) } },
        { name: "04-wait", enter: function () {} },
        { name: "05-kept", enter: function () { console.warn("walk: 05-kept count=" + Garden.countLabel + " selected=" + Garden.selectedFeedId
            + " rows=" + Garden.rowCount() + " kept6=" + (Garden.rowOfFeed(6) >= 0)) } },
        { name: "06-confirm", enter: function () { Recette.key(Qt.Key_R, "R", Qt.ShiftModifier)
            console.warn("walk: 06-confirm confirming=" + Garden.confirming + " question=" + Garden.applyQuestion
                + " preview=" + Garden.applyPreview + " confirm=" + Garden.applyConfirmLabel) } },
        { name: "07-apply", enter: function () { readingLoader.item.garden.acceptRepairs() } },
        { name: "08-wait", enter: function () {} },
        { name: "09-applied", enter: function () { console.warn("walk: 09-applied count=" + Garden.countLabel + " selected=" + Garden.selectedFeedId) } },
        { name: "10-undo", enter: function () { Recette.key(Qt.Key_U, "u", 0) } },
        { name: "11-wait", enter: function () {} },
        { name: "12-undone", enter: function () { console.warn("walk: 12-undone count=" + Garden.countLabel + " selected=" + Garden.selectedFeedId) } },
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
    readonly property var steps: Recette.walk === "journal" ? journalSteps : Recette.walk === "garden" ? gardenSteps : Recette.walk === "mark-all" ? markAllSteps : walkSteps
    property bool pressedDefault: false          // the test hook (Recette.pressDefault) pressed "Use Default Database"
    property int walkIndex: -2                   // -2 not started, -1 launch screen, 0… steps
    function settled() { return Launcher.phase === Launcher.Ready || Launcher.phase === Launcher.TooNew || Launcher.phase === Launcher.Failed }
    function slug() { return Launcher.phase === Launcher.Ready ? "ready" : Launcher.phase === Launcher.TooNew ? "too-new" : "failed" }
    function startWalkIfSettled() { if (Recette.active && walkIndex === -2 && settled()) { walkIndex = -1; walkTimer.start() } }
    Connections { target: Launcher; function onPhaseChanged() { root.startWalkIfSettled() } }
    Timer {
        id: walkTimer; interval: 400
        onTriggered: {
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

    menuBar: MainMenu { id: mainMenu; page: readingLoader.item }

    pageStack.initialPage: Kirigami.Page {
        globalToolBarStyle: Kirigami.ApplicationHeaderStyle.None
        padding: 0

        Loader {
            id: readingLoader
            anchors.fill: parent
            active: Launcher.phase === Launcher.Ready
            source: Qt.resolvedUrl("ReadingPage.qml")
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
