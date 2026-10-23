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
    readonly property var walkSteps: []          // B2 onwards: { name: "02-…", enter: function () { … } }
    property int walkIndex: -2                   // -2 not started, -1 launch screen, 0… walkSteps
    function settled() { return Launcher.phase === Launcher.Ready || Launcher.phase === Launcher.TooNew || Launcher.phase === Launcher.Failed }
    function slug() { return Launcher.phase === Launcher.Ready ? "ready" : Launcher.phase === Launcher.TooNew ? "too-new" : "failed" }
    function startWalkIfSettled() { if (Recette.active && walkIndex === -2 && settled()) { walkIndex = -1; walkTimer.start() } }
    Connections { target: Launcher; function onPhaseChanged() { root.startWalkIfSettled() } }
    Timer {
        id: walkTimer; interval: 400
        onTriggered: {
            const name = root.walkIndex < 0 ? "01-" + root.slug() : root.walkSteps[root.walkIndex].name
            if (!Recette.grab(name)) { Qt.exit(1); return }
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

            Kirigami.PlaceholderMessage {
                visible: Launcher.phase === Launcher.Ready
                text: "Ressac"
                explanation: Launcher.databasePath
                Layout.fillWidth: true
            }
        }
    }
}
