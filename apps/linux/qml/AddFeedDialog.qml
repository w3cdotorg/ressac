import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.w3cdotorg.ressac

// The Add a Feed dialog (inventory §7.1). Open while `Library.addOpen`; the form lives in `Library`, which
// closes it on success. A refusal or failure shows under the address and the dialog stays open.
QQC2.Dialog {
    id: dialog

    // The Show Log button asks the window to open the log folder.
    signal openUrl(string url)

    modal: true
    anchors.centerIn: parent
    width: 420
    title: Library.addTitle
    closePolicy: QQC2.Popup.CloseOnEscape

    function accept() { if (Library.canAdd) Library.add() }

    // Return adds even when a pop-up has the focus (its own Return only counts while its list is open).
    function addOnReturn(event, combo) {
        if (combo.popup.visible)
            return
        event.accepted = true
        dialog.accept()
    }

    Connections {
        target: Library
        function onAddChanged() {
            if (Library.addOpen && !dialog.visible)
                dialog.open()
            if (!Library.addOpen && dialog.visible)
                dialog.close()
        }
    }
    onOpened: address.forceActiveFocus()
    onClosed: if (Library.addOpen) Library.cancelAdd()

    contentItem: ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        QQC2.TextField {
            id: address
            Layout.fillWidth: true
            placeholderText: Library.addressPlaceholder
            text: Library.address
            onTextEdited: Library.address = text
            enabled: !Library.adding
            focus: true
            onAccepted: dialog.accept()
        }

        RowLayout {
            visible: Library.addError !== ""
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing

            QQC2.Label {
                Layout.fillWidth: true
                text: Library.addError
                color: Kirigami.Theme.negativeTextColor
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
            }
            QQC2.Button {
                visible: Library.addShowsLog
                text: "Show Log"
                flat: true
                onClicked: dialog.openUrl(Library.logsUrl())
            }
        }

        Kirigami.FormLayout {
            Layout.fillWidth: true

            QQC2.ComboBox {
                id: themeBox
                Kirigami.FormData.label: "Theme"
                Keys.onReturnPressed: (event) => dialog.addOnReturn(event, themeBox)
                Keys.onEnterPressed: (event) => dialog.addOnReturn(event, themeBox)
                model: Library.themeNames
                currentIndex: Library.themeIndex
                onActivated: (i) => Library.themeIndex = i
                enabled: !Library.adding
            }
            QQC2.ComboBox {
                id: folderBox
                Kirigami.FormData.label: "Folder"
                Keys.onReturnPressed: (event) => dialog.addOnReturn(event, folderBox)
                Keys.onEnterPressed: (event) => dialog.addOnReturn(event, folderBox)
                model: Library.folderTitles
                currentIndex: Library.folderIndex
                onActivated: (i) => Library.folderIndex = i
                enabled: !Library.adding
            }
        }
    }

    footer: Item {
        implicitHeight: buttons.implicitHeight + Kirigami.Units.largeSpacing * 2
        implicitWidth: buttons.implicitWidth

        RowLayout {
            id: buttons
            anchors { fill: parent; leftMargin: Kirigami.Units.largeSpacing; rightMargin: Kirigami.Units.largeSpacing
                topMargin: Kirigami.Units.smallSpacing; bottomMargin: Kirigami.Units.largeSpacing }
            spacing: Kirigami.Units.smallSpacing

            QQC2.ProgressBar {
                visible: Library.adding
                implicitWidth: 120
                indeterminate: Runs.progressFraction < 0
                value: Math.max(0, Runs.progressFraction)
            }
            Item { Layout.fillWidth: true }
            QQC2.Button { text: "Cancel"; onClicked: Library.cancelAdd() }
            QQC2.Button { text: "Add"; highlighted: true; enabled: Library.canAdd; onClicked: dialog.accept() }
        }
    }
}
