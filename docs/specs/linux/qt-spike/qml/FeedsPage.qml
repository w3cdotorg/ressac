import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Kirigami.ScrollablePage {
    id: page
    property var model
    property var backend
    title: "Sources (" + (model ? model.count : 0) + ")"

    header: QQC2.Label {
        padding: Kirigami.Units.smallSpacing
        wrapMode: Text.Wrap
        text: page.model ? page.model.storeInfo : ""
        font.pointSize: Kirigami.Theme.smallFont.pointSize
        color: Kirigami.Theme.disabledTextColor
    }

    ListView {
        model: page.model
        delegate: QQC2.ItemDelegate {
            width: ListView.view.width
            contentItem: ColumnLayout {
                spacing: 0
                Kirigami.Heading { level: 4; text: model.title }
                QQC2.Label { text: model.url + "  ·  " + model.feedState; opacity: 0.7 }
            }
            onClicked: page.backend.nodeClicked(model.title)
        }
    }
}
