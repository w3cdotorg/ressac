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
        model: SourcesModel
        currentIndex: SourcesModel.selectedRow

        Connections {
            target: SourcesModel
            function onSelectionChanged() {
                list.currentIndex = SourcesModel.selectedRow
                if (list.currentIndex >= 0)
                    list.positionViewAtIndex(list.currentIndex, ListView.Contain)
            }
        }

        delegate: QQC2.ItemDelegate {
            id: delegate
            required property int index
            required property var model

            width: ListView.view.width
            highlighted: ListView.isCurrentItem
            hoverEnabled: model.kind !== "heading"
            enabled: model.selectable || model.kind === "heading"
            onClicked: if (model.kind !== "heading") SourcesModel.select(index)

            contentItem: Loader {
                sourceComponent: delegate.model.kind === "heading" ? headingContent : rowContent
            }

            Component {
                id: headingContent
                QQC2.Label {
                    text: delegate.model.title
                    font.weight: Font.Bold
                    font.capitalization: Font.MixedCase
                    elide: Text.ElideRight
                }
            }

            Component {
                id: rowContent
                RowLayout {
                    spacing: Kirigami.Units.smallSpacing

                    Item { implicitWidth: delegate.model.depth * Kirigami.Units.gridUnit; implicitHeight: 1 }

                    QQC2.ToolButton {
                        icon.name: delegate.model.expanded ? "arrow-down" : "arrow-right"
                        visible: delegate.model.expandable
                        onClicked: SourcesModel.toggleExpanded(delegate.index)
                        padding: 0
                        implicitWidth: Kirigami.Units.iconSizes.small
                        implicitHeight: Kirigami.Units.iconSizes.small
                    }

                    Kirigami.Icon {
                        source: delegate.model.icon
                        visible: delegate.model.icon !== ""
                        implicitWidth: Kirigami.Units.iconSizes.small
                        implicitHeight: Kirigami.Units.iconSizes.small
                    }

                    QQC2.Label {
                        text: delegate.model.title
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }

                    Rectangle {
                        width: 7; height: 7; radius: 3.5
                        color: delegate.model.statusColor
                        visible: delegate.model.statusColor !== ""
                        HoverHandler { id: dotHover }
                        QQC2.ToolTip.text: delegate.model.statusText
                        QQC2.ToolTip.visible: dotHover.hovered && delegate.model.statusText !== ""
                    }

                    QQC2.Label {
                        text: delegate.model.badge
                        visible: delegate.model.badge > 0
                        color: Kirigami.Theme.disabledTextColor
                    }
                }
            }

            TapHandler {
                acceptedButtons: Qt.RightButton
                enabled: delegate.model.kind === "feed"
                onTapped: { feedMenu.row = delegate.model; feedMenu.popup() }
            }
        }
    }

    // One menu for every feed row; `row` is the model of the row it was opened on.
    QQC2.Menu {
        id: feedMenu
        property var row: ({})

        QQC2.MenuItem {
            text: "Refresh"
            enabled: Runs.canRefresh
            onTriggered: Runs.refreshFeed(feedMenu.row.feedId)
        }
        QQC2.MenuItem {
            text: "Always Fetch Full Article"
            checkable: true
            checked: feedMenu.row.preferFullText === true
            onTriggered: SourcesModel.setPreferFullText(feedMenu.row.feedId, checked)
        }
        QQC2.MenuItem {
            text: "Mark All as Read"
            enabled: feedMenu.row.badge > 0
            onTriggered: EntriesModel.markAllReadIn(feedMenu.row.key)
        }
        QQC2.MenuItem {
            text: "Open Site"
            enabled: feedMenu.row.site !== undefined && feedMenu.row.site !== ""
            onTriggered: Qt.openUrlExternally(feedMenu.row.site)
        }
    }
}
