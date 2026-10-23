import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami

Kirigami.ApplicationWindow {
    id: root

    title: "Ressac"
    width: 1100
    height: 720
    minimumWidth: 900
    minimumHeight: 500
    visible: true

    pageStack.initialPage: Kirigami.Page {
        globalToolBarStyle: Kirigami.ApplicationHeaderStyle.None

        QQC2.BusyIndicator {
            anchors.centerIn: parent
            running: true
        }
    }
}
