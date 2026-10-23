import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import org.w3cdotorg.ressac

Item {
    id: page

    readonly property int progressInset: 10  // PROGRESS_INSET

    property alias allButton: allButton
    property alias unreadButton: unreadButton
    property alias sourcesPane: sourcesPane

    function syncFilter() {
        allButton.checked = !EntriesModel.unreadOnly
        unreadButton.checked = EntriesModel.unreadOnly
    }

    function reloadAll() {
        SourcesModel.reload()
        EntriesModel.reload()
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        QQC2.ToolBar {
            Layout.fillWidth: true

            RowLayout {
                anchors.fill: parent
                spacing: Kirigami.Units.smallSpacing

                Item { Layout.fillWidth: true }

                Kirigami.SearchField {
                    id: search
                    placeholderText: "Search"
                    text: EntriesModel.searchText
                    onTextChanged: EntriesModel.searchText = text
                    Keys.onEscapePressed: text = ""
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 14
                }

                RowLayout {
                    spacing: 0
                    enabled: !SourcesModel.isGarden

                    QQC2.ButtonGroup { id: filterGroup }

                    // `checked` is synced imperatively from the model (syncFilter), never bound: a click and the
                    // group's exclusive uncheck write it directly. A click writes the model through onToggled.
                    QQC2.ToolButton {
                        id: allButton
                        text: "All"
                        checkable: true
                        QQC2.ButtonGroup.group: filterGroup
                        onToggled: if (checked) EntriesModel.unreadOnly = false
                    }

                    QQC2.ToolButton {
                        id: unreadButton
                        text: "Unread"
                        checkable: true
                        QQC2.ButtonGroup.group: filterGroup
                        onToggled: if (checked) EntriesModel.unreadOnly = true
                    }
                }

                QQC2.ToolButton {
                    visible: !Runs.progressVisible
                    icon.name: "view-refresh"
                    text: "Refresh"
                    display: QQC2.AbstractButton.IconOnly
                    enabled: Runs.canRefresh
                    onClicked: Runs.refreshSource(SourcesModel.selectedKey)
                    QQC2.ToolTip.text: text
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                }

                QQC2.ProgressBar {
                    id: pill
                    visible: Runs.progressVisible
                    implicitWidth: Runs.progressWidth
                    indeterminate: Runs.progressFraction < 0
                    value: Math.max(0, Runs.progressFraction)

                    TextMetrics { id: fullMetrics; font: pillLabel.font; text: Runs.progressText }

                    QQC2.Label {
                        id: pillLabel
                        anchors.fill: parent
                        anchors.leftMargin: page.progressInset
                        anchors.rightMargin: page.progressInset
                        verticalAlignment: Text.AlignVCenter
                        elide: Text.ElideRight
                        text: fullMetrics.width <= Runs.progressWidth - 2 * page.progressInset ? Runs.progressText : Runs.progressCompactText
                    }

                    HoverHandler { id: pillHover }
                    QQC2.ToolTip.text: Runs.progressText
                    QQC2.ToolTip.visible: pillHover.hovered
                    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                }
            }
        }

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            type: Kirigami.MessageType.Warning
            visible: Runs.bannerVisible
            text: Runs.bannerMessage
            showCloseButton: false
            actions: [
                Kirigami.Action {
                    text: "Show Log"
                    visible: Runs.bannerShowLog
                    onTriggered: Qt.openUrlExternally("file://" + encodeURI(Runs.logDir))
                },
                Kirigami.Action {
                    text: "Dismiss"
                    icon.name: "window-close"
                    displayHint: Kirigami.DisplayHint.IconOnly
                    onTriggered: Runs.dismissBanner()
                }
            ]
        }

        QQC2.SplitView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            orientation: Qt.Horizontal

            SourcesPane {
                id: sourcesPane
                QQC2.SplitView.minimumWidth: 180
                QQC2.SplitView.preferredWidth: 220
                QQC2.SplitView.maximumWidth: 320
            }

            EntriesPane {
                id: entriesPane
                QQC2.SplitView.minimumWidth: 260
                QQC2.SplitView.preferredWidth: 320
                QQC2.SplitView.maximumWidth: 480
            }

            Item {
                id: readerPane
                QQC2.SplitView.fillWidth: true
                QQC2.SplitView.minimumWidth: 300
            }
        }
    }

    Timer { interval: Live.pollInterval; repeat: true; running: Live.polling; onTriggered: Live.poll() }
    Timer { interval: 60000; repeat: true; running: true; onTriggered: Live.minuteTick() }

    Binding {
        target: Live
        property: "windowActive"
        value: page.Window.window !== null && page.Window.window.active && page.Window.window.visibility !== Window.Minimized
    }

    Connections { target: EntriesModel; function onUnreadOnlyChanged() { page.syncFilter() } }
    Connections { target: Live; function onInvalidated() { page.reloadAll() } }
    Connections { target: Runs; function onDatabaseChanged() { page.reloadAll() } }
    Connections { target: SourcesModel; function onWrote() { page.reloadAll() } }
    Connections { target: EntriesModel; function onWrote() { page.reloadAll() } }
    Connections { target: SourcesModel; function onBanner(m, l) { Runs.showBanner(m, l) } }
    Connections { target: EntriesModel; function onBanner(m, l) { Runs.showBanner(m, l) } }
    Connections { target: SourcesModel; function onSelectionChanged() { EntriesModel.showSource(SourcesModel.selectedKey) } }

    Component.onCompleted: {
        page.syncFilter()
        SourcesModel.reload()
        EntriesModel.showSource(SourcesModel.selectedKey)
    }
}
