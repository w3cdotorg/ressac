import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.w3cdotorg.ressac

// "No Feeds Yet" (inventory §7.2): replaces the reading page while the library has no active feed.
Item {
    id: firstRun

    signal chooseRequested()

    ColumnLayout {
        anchors.centerIn: parent
        width: Math.min(parent.width - Kirigami.Units.gridUnit * 4, Kirigami.Units.gridUnit * 30)
        spacing: Kirigami.Units.largeSpacing

        Kirigami.PlaceholderMessage {
            Layout.fillWidth: true
            icon.name: "view-pim-news"
            text: Library.noFeedsTitle
            explanation: Library.noFeedsText
            helpfulAction: Kirigami.Action {
                text: Library.chooseOpml
                enabled: !Library.busy
                onTriggered: firstRun.chooseRequested()
            }
        }

        QQC2.BusyIndicator {
            visible: Library.busy
            running: visible
            Layout.alignment: Qt.AlignHCenter
        }
    }
}
