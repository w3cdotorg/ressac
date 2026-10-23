import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import org.w3cdotorg.ressac

Item {
    id: inspector

    clip: true

    signal openUrl(string url)

    Kirigami.PlaceholderMessage {
        anchors.centerIn: parent
        width: parent.width - Kirigami.Units.gridUnit * 4
        visible: !Garden.hasSelection
        text: Garden.noFeedTitle
        explanation: Garden.noFeedText
        icon.name: "games-hint"
    }

    QQC2.ScrollView {
        anchors.fill: parent
        visible: Garden.hasSelection
        contentWidth: availableWidth

        ColumnLayout {
            id: column
            width: inspector.width - 40
            x: 20
            y: 20
            spacing: 14

            Kirigami.SelectableLabel {
                Layout.fillWidth: true
                text: Garden.title
                font.pointSize: Kirigami.Theme.defaultFont.pointSize * 1.4
                font.weight: Font.DemiBold
                wrapMode: Text.Wrap
            }

            QQC2.Label {
                Layout.fillWidth: true
                visible: text !== ""
                text: Garden.folderPath
                color: Kirigami.Theme.disabledTextColor
                elide: Text.ElideMiddle
            }

            Kirigami.Heading {
                Layout.fillWidth: true
                level: 4
                text: Garden.statusLine
                color: Garden.statusColor !== "" ? Garden.statusColor : Kirigami.Theme.disabledTextColor
                wrapMode: Text.Wrap
            }

            QQC2.Label {
                visible: Garden.buried
                text: Garden.buriedNote
                font.weight: Font.Medium
            }

            QQC2.Label {
                Layout.fillWidth: true
                visible: text !== ""
                text: Garden.reason
                wrapMode: Text.Wrap
            }

            RowLayout {
                Layout.fillWidth: true
                visible: Garden.rhythmLine !== ""

                QQC2.Label {
                    Layout.fillWidth: true
                    text: Garden.rhythmLine
                    wrapMode: Text.Wrap
                }

                QQC2.ComboBox {
                    model: Garden.rhythmChoices
                    currentIndex: Garden.rhythmIndex
                    focusPolicy: Qt.NoFocus
                    onActivated: (i) => Garden.setCadence(i)
                    QQC2.ToolTip.text: "Auto follows how often the feed publishes; a group chosen here stays until you choose Auto."
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                }
            }

            RowLayout {
                Layout.fillWidth: true
                visible: Garden.themeIndex >= 0

                QQC2.Label {
                    Layout.fillWidth: true
                    text: "Theme"
                }

                QQC2.ComboBox {
                    model: Garden.themeNames
                    currentIndex: Garden.themeIndex
                    focusPolicy: Qt.NoFocus
                    onActivated: (i) => Garden.setTheme(i)
                    QQC2.ToolTip.text: "The sea this feed sails on in the Sea Chart. None leaves it in the uncharted waters."
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                }
            }

            QQC2.Label {
                visible: Garden.evidence.length > 0
                text: "Evidence"
                font.weight: Font.DemiBold
            }

            Repeater {
                model: Garden.evidence
                QQC2.Label {
                    required property string modelData
                    Layout.fillWidth: true
                    text: "•  " + modelData
                    wrapMode: Text.Wrap
                }
            }

            QQC2.Label {
                visible: Garden.remarks.length > 0
                text: "Remarks"
                font.weight: Font.DemiBold
            }

            Repeater {
                model: Garden.remarks
                QQC2.Label {
                    required property string modelData
                    Layout.fillWidth: true
                    text: "•  " + modelData
                    wrapMode: Text.Wrap
                }
            }

            GridLayout {
                Layout.fillWidth: true
                columns: 2
                columnSpacing: Kirigami.Units.largeSpacing
                rowSpacing: Kirigami.Units.smallSpacing

                QQC2.Label {
                    visible: Garden.proposed !== ""
                    Layout.alignment: Qt.AlignRight | Qt.AlignTop
                    text: "Proposed"
                    color: Kirigami.Theme.disabledTextColor
                }
                Kirigami.SelectableLabel {
                    visible: Garden.proposed !== ""
                    Layout.fillWidth: true
                    text: Garden.proposed
                    wrapMode: Text.Wrap
                }

                QQC2.Label {
                    visible: Garden.decision !== ""
                    Layout.alignment: Qt.AlignRight | Qt.AlignTop
                    text: "Decision"
                    color: Kirigami.Theme.disabledTextColor
                }
                Kirigami.SelectableLabel {
                    visible: Garden.decision !== ""
                    Layout.fillWidth: true
                    text: Garden.decision
                    wrapMode: Text.Wrap
                }

                QQC2.Label {
                    Layout.alignment: Qt.AlignRight | Qt.AlignTop
                    text: "Feed"
                    color: Kirigami.Theme.disabledTextColor
                }
                Kirigami.SelectableLabel {
                    Layout.fillWidth: true
                    text: Garden.feedUrl
                    wrapMode: Text.WrapAnywhere
                }

                QQC2.Label {
                    visible: Garden.site !== ""
                    Layout.alignment: Qt.AlignRight | Qt.AlignTop
                    text: "Site"
                    color: Kirigami.Theme.disabledTextColor
                }
                QQC2.Label {
                    visible: Garden.site !== ""
                    Layout.fillWidth: true
                    text: "<a href=\"" + Garden.site + "\">" + Garden.site + "</a>"
                    textFormat: Text.StyledText
                    wrapMode: Text.WrapAnywhere
                    onLinkActivated: inspector.openUrl(Garden.site)
                }

                QQC2.Label {
                    visible: Garden.wayback !== ""
                    Layout.alignment: Qt.AlignRight | Qt.AlignTop
                    text: "Wayback"
                    color: Kirigami.Theme.disabledTextColor
                }
                QQC2.Label {
                    visible: Garden.wayback !== ""
                    Layout.fillWidth: true
                    text: "<a href=\"" + Garden.wayback + "\">" + Garden.archivedCopy + "</a>"
                    textFormat: Text.StyledText
                    wrapMode: Text.Wrap
                    onLinkActivated: inspector.openUrl(Garden.wayback)
                }
            }

            Flow {
                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing

                component KeyedButton: QQC2.Button {
                    id: keyed
                    property string title
                    property string key
                    leftPadding: 10
                    rightPadding: 10
                    focusPolicy: Qt.NoFocus
                    contentItem: Row {
                        spacing: 4
                        QQC2.Label { text: keyed.title; color: keyed.palette.buttonText }
                        QQC2.Label { text: keyed.key; font: Kirigami.Theme.smallFont; opacity: 0.6; color: keyed.palette.buttonText }
                    }
                }

                KeyedButton { title: Garden.keepTitle; key: "k"; onClicked: Garden.decide("keep") }
                KeyedButton { title: "Repair"; key: "r"; visible: Garden.canRepair; highlighted: true; onClicked: Garden.decide("repair") }
                KeyedButton { title: "Bury"; key: "b"; visible: Garden.canBury; highlighted: Garden.buryProminent; onClicked: Garden.decide("bury") }
                KeyedButton { title: "Open Site"; key: "o"; enabled: Garden.site !== ""; onClicked: inspector.openUrl(Garden.site) }
                KeyedButton { title: "Wayback"; key: "w"; enabled: Garden.wayback !== ""; onClicked: inspector.openUrl(Garden.wayback) }
            }

            QQC2.ToolButton {
                id: disclosure
                property bool open: false
                text: Garden.historyTitle
                icon.name: open ? "arrow-down" : "arrow-right"
                focusPolicy: Qt.NoFocus
                onClicked: open = !open
            }

            ColumnLayout {
                Layout.fillWidth: true
                visible: disclosure.open
                spacing: 6

                QQC2.Label {
                    visible: Garden.redirects.length > 0
                    text: "Redirects"
                    font.weight: Font.DemiBold
                }

                Kirigami.SelectableLabel {
                    Layout.fillWidth: true
                    visible: Garden.redirects.length > 0
                    text: Garden.redirects.join("\n")
                    font.family: "monospace"
                    font.pointSize: Kirigami.Theme.smallFont.pointSize
                    wrapMode: Text.WrapAnywhere
                }

                QQC2.Label {
                    visible: Garden.logDates.length === 0
                    text: Garden.notFetched
                    color: Kirigami.Theme.disabledTextColor
                }

                QQC2.Label {
                    visible: Garden.logDates.length > 0
                    text: "Fetch log"
                    font.weight: Font.DemiBold
                }

                Repeater {
                    model: Garden.logDates

                    RowLayout {
                        required property int index
                        required property string modelData
                        Layout.fillWidth: true
                        spacing: Kirigami.Units.largeSpacing

                        QQC2.Label {
                            text: modelData
                            font.family: Kirigami.Theme.smallFont.family
                            font.pointSize: Kirigami.Theme.smallFont.pointSize
                            font.features: { "tnum": 1 }
                            color: Kirigami.Theme.disabledTextColor
                        }
                        QQC2.Label {
                            Layout.fillWidth: true
                            text: Garden.logOutcomes[index] ?? ""
                            font: Kirigami.Theme.smallFont
                            wrapMode: Text.Wrap
                        }
                    }
                }
            }

            Item { Layout.preferredHeight: 20 }
        }
    }
}
