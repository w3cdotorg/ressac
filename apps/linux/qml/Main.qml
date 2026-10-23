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
            console.warn("walk: 07-unread-filter clicked=" + clicked + " all=" + page.allButton.checked + " unread=" + page.unreadButton.checked) } }
    ]
    property bool pressedDefault: false          // the test hook (Recette.pressDefault) pressed "Use Default Database"
    property int walkIndex: -2                   // -2 not started, -1 launch screen, 0… walkSteps
    function settled() { return Launcher.phase === Launcher.Ready || Launcher.phase === Launcher.TooNew || Launcher.phase === Launcher.Failed }
    function slug() { return Launcher.phase === Launcher.Ready ? "ready" : Launcher.phase === Launcher.TooNew ? "too-new" : "failed" }
    function startWalkIfSettled() { if (Recette.active && walkIndex === -2 && settled()) { walkIndex = -1; walkTimer.start() } }
    Connections { target: Launcher; function onPhaseChanged() { root.startWalkIfSettled() } }
    Timer {
        id: walkTimer; interval: 400
        onTriggered: {
            const name = root.walkIndex < 0 ? (root.pressedDefault ? "02-" : "01-") + root.slug() : root.walkSteps[root.walkIndex].name
            if (!Recette.grab(name)) { Qt.exit(1); return }
            if (root.walkIndex === -1 && !root.pressedDefault && Recette.pressDefault !== "" && root.settled() && Launcher.phase !== Launcher.Ready) {
                root.pressedDefault = true; root.walkIndex = -2
                Recette.prepareDefault(); root.useDefaultDatabase()
                return
            }
            root.walkIndex += 1
            if (Launcher.phase !== Launcher.Ready || root.walkIndex >= root.walkSteps.length) { Qt.exit(0); return }
            root.walkSteps[root.walkIndex].enter()
            walkTimer.start()
        }
    }
    Timer { interval: 60000; running: Recette.active; onTriggered: { console.error("ressac-kde: the screenshot walk timed out"); Qt.exit(1) } }

    Component.onCompleted: { Launcher.launch(); root.startWalkIfSettled() }

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
