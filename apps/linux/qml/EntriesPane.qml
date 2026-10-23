import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import org.w3cdotorg.ressac

Item {
    id: pane

    ListView {
        id: list
        anchors.fill: parent
        clip: true
        visible: EntriesModel.active
        model: EntriesModel
        currentIndex: EntriesModel.selectedRow

        Connections {
            target: EntriesModel
            function onSelectionChanged() {
                list.currentIndex = EntriesModel.selectedRow
                if (list.currentIndex >= 0)
                    list.positionViewAtIndex(list.currentIndex, ListView.Contain)
            }
        }

        delegate: QQC2.ItemDelegate {
            id: delegate
            required property int index
            required property var model

            readonly property color dimColor: highlighted ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.disabledTextColor
            width: ListView.view.width
            highlighted: ListView.isCurrentItem
            onClicked: EntriesModel.select(index)
            Component.onCompleted: EntriesModel.rowAppeared(index)

            contentItem: RowLayout {
                spacing: Kirigami.Units.smallSpacing

                Rectangle {
                    width: 8; height: 8; radius: 4
                    color: delegate.model.read ? "transparent" : (delegate.highlighted ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.highlightColor)
                    Layout.alignment: Qt.AlignTop
                    Layout.topMargin: Kirigami.Units.smallSpacing
                }

                ColumnLayout {
                    spacing: 2
                    Layout.fillWidth: true

                    RowLayout {
                        spacing: Kirigami.Units.smallSpacing
                        Layout.fillWidth: true

                        QQC2.Label {
                            text: delegate.model.feedTitle
                            font: Kirigami.Theme.smallFont
                            color: delegate.dimColor
                            elide: Text.ElideRight
                            Layout.fillWidth: true
                        }
                        Kirigami.Icon {
                            source: "starred-symbolic"
                            visible: delegate.model.starred
                            implicitWidth: Kirigami.Units.iconSizes.small
                            implicitHeight: Kirigami.Units.iconSizes.small
                        }
                        QQC2.Label {
                            text: delegate.model.date
                            font: Kirigami.Theme.smallFont
                            color: delegate.dimColor
                        }
                    }

                    QQC2.Label {
                        text: delegate.model.title
                        maximumLineCount: 2
                        wrapMode: Text.Wrap
                        elide: Text.ElideRight
                        font.weight: delegate.model.read ? Font.Normal : Font.DemiBold
                        color: delegate.highlighted ? Kirigami.Theme.highlightedTextColor : (delegate.model.read ? Kirigami.Theme.disabledTextColor : Kirigami.Theme.textColor)
                        Layout.fillWidth: true
                    }

                    QQC2.Label {
                        text: delegate.model.excerpt
                        visible: text !== ""
                        maximumLineCount: 2
                        wrapMode: Text.Wrap
                        elide: Text.ElideRight
                        font: Kirigami.Theme.smallFont
                        color: delegate.dimColor
                        Layout.fillWidth: true
                    }
                }
            }
        }
    }

    Kirigami.PlaceholderMessage {
        anchors.centerIn: parent
        width: parent.width - Kirigami.Units.gridUnit * 2
        visible: EntriesModel.active && list.count === 0 && EntriesModel.emptyText !== ""
        text: EntriesModel.emptyText
    }
}
