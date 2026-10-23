import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import org.w3cdotorg.ressac

Item {
    id: pane

    clip: true

    function focusList() { list.forceActiveFocus() }
    function acceptRepairs() { repairs.accept() }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        RowLayout {
            Layout.fillWidth: true
            Layout.preferredHeight: 44
            Layout.leftMargin: 12
            Layout.rightMargin: 12
            spacing: Kirigami.Units.smallSpacing

            Kirigami.Heading {
                level: 4
                text: Garden.scopeTitle
            }

            Rectangle {
                implicitWidth: countLabel.implicitWidth + 14
                implicitHeight: countLabel.implicitHeight + 4
                radius: height / 2
                color: Kirigami.Theme.alternateBackgroundColor

                QQC2.Label {
                    id: countLabel
                    anchors.centerIn: parent
                    text: Garden.countLabel
                    font.family: Kirigami.Theme.smallFont.family
                    font.pointSize: Kirigami.Theme.smallFont.pointSize
                    font.features: { "tnum": 1 }
                }
            }

            RowLayout {
                visible: Garden.showsGrouping
                spacing: 0

                QQC2.ButtonGroup { id: groupingGroup }

                Repeater {
                    model: Garden.groupingTitles

                    QQC2.ToolButton {
                        required property int index
                        required property string modelData
                        readonly property string value: index === 0 ? "status" : "rhythm"
                        text: modelData
                        checkable: true
                        checked: Garden.grouping === value
                        focusPolicy: Qt.NoFocus
                        QQC2.ButtonGroup.group: groupingGroup
                        onClicked: Garden.grouping = value
                        QQC2.ToolTip.text: "Group the feeds by status or by publishing rhythm"
                        QQC2.ToolTip.visible: hovered
                        QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                    }
                }
            }

            QQC2.Label {
                Layout.fillWidth: true
                horizontalAlignment: Text.AlignRight
                text: Garden.message
                font: Kirigami.Theme.smallFont
                color: Kirigami.Theme.disabledTextColor
                elide: Text.ElideRight
                opacity: Garden.message !== "" ? 1 : 0
                Behavior on opacity { NumberAnimation { duration: 150 } }
            }

            QQC2.Button {
                text: Garden.applyTitle
                highlighted: true
                visible: Garden.showsApply
                enabled: Garden.safeRepairCount > 0
                focusPolicy: Qt.NoFocus
                onClicked: Garden.confirming = true
                QQC2.ToolTip.text: "Apply every safe repair: https upgrades and permanent redirects (Shift+R)"
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            }

            QQC2.Button {
                focusPolicy: Qt.NoFocus
                action: QQC2.Action {
                    text: "Undo"
                    shortcut: StandardKey.Undo
                    enabled: Garden.active
                    onTriggered: Garden.undo()
                }
                QQC2.ToolTip.text: "Undo the last decision, here or in the terminal garden (U or Ctrl+Z)"
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            }
        }

        Kirigami.Separator { Layout.fillWidth: true }

        ListView {
            id: list
            objectName: "gardenList"
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            focus: true
            model: Garden
            currentIndex: Garden.selectedRow

            section.property: "section"
            section.delegate: Kirigami.ListSectionHeader {
                required property string section
                text: section
            }

            delegate: QQC2.ItemDelegate {
                id: delegate
                required property int index
                required property var model

                width: ListView.view.width
                highlighted: ListView.isCurrentItem
                focusPolicy: Qt.NoFocus
                opacity: model.dimmed ? 0.45 : 1
                Behavior on opacity { NumberAnimation { duration: 200 } }
                QQC2.ToolTip.text: model.tooltip
                QQC2.ToolTip.visible: hovered && model.tooltip !== ""
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay

                contentItem: RowLayout {
                    spacing: Kirigami.Units.smallSpacing

                    QQC2.Label {
                        text: delegate.model.glyph
                        horizontalAlignment: Text.AlignHCenter
                        Layout.preferredWidth: 22
                    }

                    QQC2.Label {
                        text: delegate.model.title
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }

                    QQC2.Label {
                        text: delegate.model.clue
                        font.features: { "tnum": 1 }
                        color: Kirigami.Theme.disabledTextColor
                    }

                    QQC2.Label {
                        id: rhythmLabel
                        text: delegate.model.rhythm
                        color: Kirigami.Theme.disabledTextColor
                        HoverHandler { id: rhythmHover }
                        QQC2.ToolTip.text: text.endsWith("*") ? "Rhythm set by you" : "Rhythm"
                        QQC2.ToolTip.visible: rhythmHover.hovered && text !== ""
                        QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                    }

                    QQC2.Label {
                        text: delegate.model.unread
                        color: Kirigami.Theme.disabledTextColor
                    }

                    Rectangle {
                        visible: delegate.model.chip !== ""
                        implicitWidth: chipLabel.implicitWidth + 12
                        implicitHeight: chipLabel.implicitHeight + 4
                        radius: height / 2
                        color: Kirigami.Theme.alternateBackgroundColor

                        QQC2.Label {
                            id: chipLabel
                            anchors.centerIn: parent
                            text: delegate.model.chip
                            font: Kirigami.Theme.smallFont
                        }
                    }
                }

                onClicked: { Garden.select(index); list.forceActiveFocus() }
            }

            Connections {
                target: Garden
                function onSelectionChanged() {
                    list.currentIndex = Garden.selectedRow
                    if (list.currentIndex >= 0)
                        list.positionViewAtIndex(list.currentIndex, ListView.Contain)
                }
            }

            // The arrows (ListView's own key navigation) move currentIndex, which selects.
            onCurrentIndexChanged: if (currentIndex !== Garden.selectedRow && currentIndex >= 0) Garden.select(currentIndex)

            Keys.onPressed: (event) => { event.accepted = Garden.key(event.text, event.modifiers) }
        }
    }

    Kirigami.PlaceholderMessage {
        anchors.centerIn: parent
        width: parent.width - Kirigami.Units.gridUnit * 4
        visible: Garden.emptyText !== ""
        text: Garden.emptyText
        icon.name: "games-hint"
    }

    QQC2.Dialog {
        id: repairs
        modal: true
        anchors.centerIn: parent
        title: Garden.applyQuestion
        width: Math.min(pane.width - Kirigami.Units.gridUnit * 4, Kirigami.Units.gridUnit * 28)

        QQC2.Label {
            width: parent.width
            text: Garden.applyPreview
            wrapMode: Text.Wrap
        }

        footer: QQC2.DialogButtonBox {
            QQC2.Button {
                text: Garden.applyConfirmLabel
                QQC2.DialogButtonBox.buttonRole: QQC2.DialogButtonBox.AcceptRole
            }
            QQC2.Button {
                text: "Cancel"
                QQC2.DialogButtonBox.buttonRole: QQC2.DialogButtonBox.RejectRole
            }
        }

        onAccepted: Garden.applySafe()
        onClosed: Garden.confirming = false
    }

    Connections {
        target: Garden
        function onStateChanged() { if (Garden.confirming && !repairs.visible) repairs.open() }
    }
}
