import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import org.w3cdotorg.ressac

Item {
    id: pane

    readonly property bool journal: EntriesModel.journal
    // The Journal's placeholder replaces the list (building, offline, no edition ...).
    readonly property bool showPlaceholder: journal && Journal.placeholder !== ""

    function focusList() { list.forceActiveFocus() }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0
        visible: EntriesModel.active && !pane.showPlaceholder

        QQC2.Label {
            visible: pane.journal && Journal.placeholder === "" && Journal.header !== ""
            text: Journal.header
            font.family: Kirigami.Theme.smallFont.family
            font.pointSize: Kirigami.Theme.smallFont.pointSize
            font.features: { "tnum": 1 }
            color: Kirigami.Theme.disabledTextColor
            wrapMode: Text.Wrap
            leftPadding: 12; rightPadding: 12; topPadding: 6; bottomPadding: 6
            Layout.fillWidth: true
        }

        Kirigami.Separator {
            visible: pane.journal && Journal.placeholder === "" && Journal.header !== ""
            Layout.fillWidth: true
        }

        ListView {
            id: list
            objectName: "entriesList"
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            keyNavigationEnabled: false        // the window's key filter moves the selection
            model: EntriesModel
            currentIndex: EntriesModel.selectedRow

            section.property: pane.journal ? "section" : ""
            section.delegate: Item {
                id: sectionItem
                required property string section
                // Plain slots (an unreadable edition) carry no section.
                width: ListView.view.width
                height: section === "" ? 0 : header.implicitHeight
                visible: section !== ""
                Kirigami.ListSectionHeader {
                    id: header
                    width: parent.width
                    text: sectionItem.section
                }
            }

            footer: Item {
                readonly property bool shown: pane.journal && Journal.finished
                width: ListView.view.width
                height: shown ? endLine.implicitHeight : 0
                visible: shown

                QQC2.Label {
                    id: endLine
                    width: parent.width
                    text: Journal.endLine
                    color: Kirigami.Theme.disabledTextColor
                    horizontalAlignment: Text.AlignHCenter
                    padding: 8
                }
            }

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

                readonly property bool isOverflow: model.kind === "overflow"
                readonly property color dimColor: highlighted ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.disabledTextColor
                width: ListView.view.width
                highlighted: !isOverflow && ListView.isCurrentItem
                focusPolicy: Qt.NoFocus
                onClicked: {
                    if (isOverflow) {
                        SourcesModel.selectKey("feed:" + model.overflowFeed)
                    } else {
                        list.forceActiveFocus()
                        EntriesModel.select(index)
                    }
                }
                Component.onCompleted: if (!isOverflow) EntriesModel.rowAppeared(index)

                contentItem: ColumnLayout {
                    spacing: 0

                    // "+N more in Feed": one small grey line, never highlighted.
                    QQC2.Label {
                        visible: delegate.isOverflow
                        text: delegate.model.title
                        font: Kirigami.Theme.smallFont
                        color: Kirigami.Theme.disabledTextColor
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }

                    RowLayout {
                        visible: !delegate.isOverflow
                        spacing: Kirigami.Units.smallSpacing
                        Layout.fillWidth: true

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
        }
    }

    // The Journal's placeholder: what the edition is waiting for, with Retry when it failed.
    ColumnLayout {
        anchors.centerIn: parent
        width: parent.width - Kirigami.Units.gridUnit * 2
        spacing: Kirigami.Units.largeSpacing
        visible: EntriesModel.active && pane.showPlaceholder

        QQC2.BusyIndicator {
            visible: Journal.building
            running: visible
            Layout.alignment: Qt.AlignHCenter
        }
        QQC2.Label {
            text: Journal.placeholder
            color: Kirigami.Theme.disabledTextColor
            wrapMode: Text.Wrap
            horizontalAlignment: Text.AlignHCenter
            Layout.fillWidth: true
        }
        QQC2.Button {
            visible: Journal.offersRetry
            text: Journal.retryTitle
            onClicked: Journal.retry()
            Layout.alignment: Qt.AlignHCenter
        }
    }

    Kirigami.PlaceholderMessage {
        anchors.centerIn: parent
        width: parent.width - Kirigami.Units.gridUnit * 2
        visible: EntriesModel.active && !pane.showPlaceholder && list.count === 0 && EntriesModel.emptyText !== ""
        text: EntriesModel.emptyText
    }
}
