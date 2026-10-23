import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import org.w3cdotorg.ressac

Item {
    id: pane

    function focusList() { list.forceActiveFocus() }

    ListView {
        id: list
        objectName: "sourcesList"
        anchors.fill: parent
        clip: true
        focus: true
        keyNavigationEnabled: false        // the arrows select (below); currentIndex follows the model
        model: SourcesModel
        currentIndex: SourcesModel.selectedRow

        // ↑ and ↓ select the previous and next row, skipping headings (select leaves selectedRow alone for them).
        function step(direction) {
            let i = SourcesModel.selectedRow
            if (i < 0 && direction < 0)
                i = count
            for (i += direction; i >= 0 && i < count; i += direction) {
                SourcesModel.select(i)
                if (SourcesModel.selectedRow === i)
                    return
            }
        }
        Keys.onUpPressed: step(-1)
        Keys.onDownPressed: step(1)

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
            focusPolicy: Qt.NoFocus
            hoverEnabled: model.kind !== "heading"
            enabled: model.selectable || model.kind === "heading"
            onClicked: {
                list.forceActiveFocus()
                if (model.kind !== "heading") SourcesModel.select(index)
            }

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
                onTapped: {
                    feedMenu.prepare({ feedId: delegate.model.feedId, key: delegate.model.key, badge: delegate.model.badge,
                                       site: delegate.model.site, preferFullText: delegate.model.preferFullText })
                    feedMenu.popup()
                }
            }
        }
    }

    property alias feedMenu: feedMenu

    // One menu for every feed row. `prepare` copies the row's values into plain properties (the
    // delegate's model may go stale) and sets the checkmark by hand (a toggle would break a binding).
    QQC2.Menu {
        id: feedMenu
        property int feedId: 0
        property string key: ""
        property int badge: 0
        property string site: ""
        property alias fullTextItem: fullTextItem

        function prepare(row) {
            feedId = row.feedId; key = row.key; badge = row.badge; site = row.site
            fullTextItem.checked = row.preferFullText
        }

        QQC2.MenuItem {
            text: "Refresh"
            enabled: Runs.canRefresh
            onTriggered: Runs.refreshFeed(feedMenu.feedId)
        }
        QQC2.MenuItem {
            id: fullTextItem
            text: "Always Fetch Full Article"
            checkable: true
            onTriggered: SourcesModel.setPreferFullText(feedMenu.feedId, checked)
        }
        QQC2.MenuItem {
            text: "Mark All as Read"
            enabled: feedMenu.badge > 0
            onTriggered: EntriesModel.markAllReadIn(feedMenu.key)
        }
        QQC2.MenuItem {
            text: "Open Site"
            enabled: feedMenu.site !== ""
            onTriggered: Qt.openUrlExternally(feedMenu.site)
        }
        QQC2.MenuItem {
            text: "Show in Garden"
            onTriggered: { SourcesModel.selectKey("garden:all"); Garden.selectFeed(feedMenu.feedId) }
        }
    }
}
