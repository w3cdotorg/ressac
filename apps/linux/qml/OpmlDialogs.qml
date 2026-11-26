import QtQuick
import QtCore
import QtQuick.Controls as QQC2
import QtQuick.Dialogs
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.w3cdotorg.ressac

// The OPML dialogs (inventory §7.2): the two file dialogs, the import confirmation and the failure alert.
// The logic is in `Library`; a walk calls the functions below since a FileDialog cannot be driven offscreen.
Item {
    id: opml

    function chooseImport() { if (!Library.busy) importFile.open() }
    function chooseExport() {
        exportFile.currentFolder = StandardPaths.writableLocation(StandardPaths.DocumentsLocation)
        exportFile.selectedFile = exportFile.currentFolder + "/" + Library.exportName
        exportFile.open()
    }
    function confirmImport() { confirm.accept() }

    FileDialog {
        id: importFile
        nameFilters: [Library.opmlFilter]
        onAccepted: Library.openUrls([String(selectedFile)])
    }

    FileDialog {
        id: exportFile
        fileMode: FileDialog.SaveFile
        defaultSuffix: "opml"
        nameFilters: [Library.opmlFilter]
        onAccepted: Library.exportTo(String(selectedFile))
    }

    Connections {
        target: Library
        function onOpmlChanged() {
            const confirming = Library.opmlStep === "confirming"
            if (confirming && !confirm.visible)
                confirm.open()
            if (!confirming && confirm.visible)
                confirm.close()
            const failed = Library.opmlStep === "failed"
            if (failed && !alert.visible)
                alert.open()
            if (!failed && alert.visible)
                alert.close()
        }
    }

    QQC2.Dialog {
        id: confirm
        modal: true
        anchors.centerIn: parent
        width: 420
        title: Library.importTitle
        closePolicy: QQC2.Popup.CloseOnEscape

        onAccepted: Library.confirmImport(Runs.longRunActive)
        onRejected: Library.cancelImport()

        contentItem: ColumnLayout {
            spacing: Kirigami.Units.smallSpacing

            Repeater {
                model: Library.importLines
                QQC2.Label {
                    required property string modelData
                    Layout.fillWidth: true
                    text: modelData
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                }
            }
            QQC2.Label {
                visible: Library.importNote !== ""
                Layout.fillWidth: true
                text: Library.importNote
                font: Kirigami.Theme.smallFont
                color: Kirigami.Theme.disabledTextColor
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
            }
            QQC2.Label {
                visible: Runs.longRunActive
                Layout.fillWidth: true
                text: Library.importBlocked
                font: Kirigami.Theme.smallFont
                color: Kirigami.Theme.disabledTextColor
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
            }
        }

        footer: QQC2.DialogButtonBox {
            QQC2.Button { text: "Cancel"; QQC2.DialogButtonBox.buttonRole: QQC2.DialogButtonBox.RejectRole }
            QQC2.Button {
                text: "Import"
                QQC2.DialogButtonBox.buttonRole: QQC2.DialogButtonBox.AcceptRole
                enabled: Library.canImport && !Runs.longRunActive
            }
        }
    }

    QQC2.Dialog {
        id: alert
        modal: true
        anchors.centerIn: parent
        width: 420
        title: Library.cantImport
        closePolicy: QQC2.Popup.CloseOnEscape

        onClosed: Library.cancelImport()

        contentItem: QQC2.Label {
            text: Library.failure
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
        }

        footer: QQC2.DialogButtonBox {
            QQC2.Button { text: "OK"; QQC2.DialogButtonBox.buttonRole: QQC2.DialogButtonBox.AcceptRole }
        }
    }
}
