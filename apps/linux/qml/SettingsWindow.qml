import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import QtQuick.Dialogs
import org.kde.kirigami as Kirigami
import org.w3cdotorg.ressac

// Settings (inventory §7.3): Reading (appearance, font, size, line width, preview) and Data (database,
// relaunch, logs).
QQC2.ApplicationWindow {
    id: settings
    title: "Settings"
    width: 560
    minimumWidth: 540
    height: 520
    visible: false

    // Main.qml's window (the log folder opens through it).
    property var chrome: null

    readonly property int fontIndex: Math.max(0, AppSettings.fontIds.indexOf(AppSettings.readerFont))
    // The look shown (Native whenever Bedstead did not load) and Antiope's four text sizes.
    readonly property bool antiope: AppSettings.effectiveAppearance === "antiope"
    readonly property var antiopeSizes: [15, 20, 25, 30]
    readonly property string previewFamily: AppSettings.fontFamilies[fontIndex] || Kirigami.Theme.defaultFont.family

    property alias sizeSlider: sizeSlider
    property alias antiopeSlider: antiopeSlider
    property alias previewLabel: previewLabel

    function showPage(name) { tabs.currentIndex = name === "data" ? 1 : 0 }

    // Quits only once the new instance has started (the Mac opens a new instance and terminates).
    function relaunchNow() {
        if (Recette.active) { console.warn("settings: relaunch"); return }
        if (AppSettings.relaunch()) Qt.quit()
    }

    function showLogs() { if (chrome) chrome.openExternally(Library.logsUrl()) }

    function folderOfDatabase() {
        const path = AppSettings.resolvedDatabasePath
        const slash = path.lastIndexOf("/")
        return "file://" + encodeURI(slash > 0 ? path.substring(0, slash) : "/")
    }

    // The tabs and every page keep a margin from the window's edges (the Mac's Form insets).
    header: QQC2.TabBar {
        id: tabs
        leftPadding: Kirigami.Units.largeSpacing
        rightPadding: Kirigami.Units.largeSpacing
        topPadding: Kirigami.Units.smallSpacing
        QQC2.TabButton { text: "Reading" }
        QQC2.TabButton { text: "Data" }
    }

    // A StackLayout ignores its pages' `Layout.margins`: the margin is the StackLayout's own.
    StackLayout {
        anchors.fill: parent
        anchors.margins: Kirigami.Units.largeSpacing
        currentIndex: tabs.currentIndex

        Kirigami.FormLayout {
            id: reading

            ColumnLayout {
                Kirigami.FormData.label: "Appearance:"
                spacing: Kirigami.Units.smallSpacing

                RowLayout {
                    spacing: 0
                    enabled: AppSettings.bedsteadAvailable
                    QQC2.ButtonGroup { id: appearanceGroup }
                    QQC2.ToolButton {
                        objectName: "appearanceNative"
                        text: "Native"
                        checkable: true
                        QQC2.ButtonGroup.group: appearanceGroup
                        checked: !settings.antiope
                        onClicked: AppSettings.appearance = "native"
                    }
                    QQC2.ToolButton {
                        objectName: "appearanceAntiope"
                        text: "Antiope"
                        checkable: true
                        QQC2.ButtonGroup.group: appearanceGroup
                        checked: settings.antiope
                        onClicked: AppSettings.appearance = "antiope"
                    }
                }
                QQC2.Label {
                    visible: !AppSettings.bedsteadAvailable
                    text: "Antiope needs the Bedstead font, which did not load."
                    font: Kirigami.Theme.smallFont
                    color: Kirigami.Theme.disabledTextColor
                    wrapMode: Text.Wrap
                    Layout.fillWidth: true
                }
            }

            ColumnLayout {
                Kirigami.FormData.label: "Font:"
                spacing: Kirigami.Units.smallSpacing

                QQC2.ComboBox {
                    enabled: !settings.antiope
                    model: AppSettings.fontTitles
                    currentIndex: settings.fontIndex
                    onActivated: (i) => AppSettings.readerFont = AppSettings.fontIds[i]
                    delegate: QQC2.ItemDelegate {
                        required property int index
                        required property var modelData
                        width: ListView.view ? ListView.view.width : implicitWidth
                        text: modelData
                        font.family: AppSettings.fontFamilies[index] || Kirigami.Theme.defaultFont.family
                        highlighted: index === settings.fontIndex
                    }
                }
                QQC2.Label {
                    visible: settings.antiope
                    text: "Bedstead in Antiope"
                    font: Kirigami.Theme.smallFont
                    color: Kirigami.Theme.disabledTextColor
                }
            }

            RowLayout {
                Kirigami.FormData.label: "Text size:"
                spacing: Kirigami.Units.smallSpacing
                Kirigami.Icon { source: "format-font-size-less"; implicitWidth: Kirigami.Units.iconSizes.small; implicitHeight: Kirigami.Units.iconSizes.small }
                QQC2.Slider {
                    id: sizeSlider
                    visible: !settings.antiope
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 10
                    Kirigami.StyleHints.tickMarkStepSize: -1      // no tick per point (the Mac's slider has none)
                    from: 12; to: 28; stepSize: 1
                    snapMode: QQC2.Slider.SnapAlways
                    value: AppSettings.readerSize
                    onMoved: AppSettings.readerSize = value
                }
                // Antiope draws at one of four sizes of its own.
                QQC2.Slider {
                    id: antiopeSlider
                    objectName: "antiopeSize"
                    visible: settings.antiope
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 10
                    Kirigami.StyleHints.tickMarkStepSize: 1       // a tick on each of the four sizes
                    from: 0; to: 3; stepSize: 1
                    snapMode: QQC2.Slider.SnapAlways
                    value: settings.antiopeSizes.indexOf(AppSettings.readerSizeAntiope)
                    onMoved: AppSettings.readerSizeAntiope = settings.antiopeSizes[value]
                }
                Kirigami.Icon { source: "format-font-size-more"; implicitWidth: Kirigami.Units.iconSizes.small; implicitHeight: Kirigami.Units.iconSizes.small }
                QQC2.Label {
                    text: Math.round(settings.antiope ? AppSettings.readerSizeAntiope : AppSettings.readerSize) + " pt"
                    font.features: { "tnum": 1 }
                    color: Kirigami.Theme.disabledTextColor
                }
            }

            RowLayout {
                Kirigami.FormData.label: "Line width:"
                spacing: 0
                QQC2.ButtonGroup { id: widthGroup }
                Repeater {
                    model: AppSettings.widthTitles
                    QQC2.ToolButton {
                        required property int index
                        required property var modelData
                        text: modelData
                        checkable: true
                        QQC2.ButtonGroup.group: widthGroup
                        checked: AppSettings.widthIds[index] === AppSettings.readerWidth
                        onClicked: AppSettings.readerWidth = AppSettings.widthIds[index]
                    }
                }
            }

            QQC2.Label {
                id: previewLabel
                Kirigami.FormData.label: "Preview:"
                text: AppSettings.previewText
                font.family: settings.antiope ? "Bedstead" : settings.previewFamily
                font.pixelSize: Math.round(settings.antiope ? AppSettings.readerSizeAntiope : AppSettings.readerSize)
                wrapMode: Text.Wrap
                maximumLineCount: 3
                elide: Text.ElideRight
                // A FormLayout centres its content and sizes it by its widest field: the preview's own width
                // (it follows the text size) must not decide it, or the whole form jumps as the slider moves.
                Layout.preferredWidth: Kirigami.Units.gridUnit * 24
                Layout.fillWidth: true
            }

            QQC2.Label {
                text: AppSettings.sizeHint
                font: Kirigami.Theme.smallFont
                color: Kirigami.Theme.disabledTextColor
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
        }

        Kirigami.FormLayout {
            id: data

            ColumnLayout {
                Kirigami.FormData.label: "Database:"
                spacing: Kirigami.Units.smallSpacing
                Layout.fillWidth: true

                QQC2.TextField {
                    readOnly: true
                    text: AppSettings.resolvedDatabasePath
                    Layout.fillWidth: true
                    QQC2.ToolTip.text: text
                    QQC2.ToolTip.visible: hovered && text !== ""
                }
                RowLayout {
                    spacing: Kirigami.Units.smallSpacing
                    QQC2.Button { text: "Choose…"; onClicked: chooseDb.open() }
                    QQC2.Button {
                        text: "New…"
                        onClicked: {
                            newDb.currentFolder = settings.folderOfDatabase()
                            newDb.selectedFile = newDb.currentFolder + "/" + AppSettings.newName
                            newDb.open()
                        }
                    }
                    QQC2.Button {
                        text: "Use Default"
                        enabled: AppSettings.databasePath !== ""
                        onClicked: AppSettings.useDefaultLocation()
                    }
                }
                QQC2.Label {
                    text: AppSettings.databaseNote
                    font: Kirigami.Theme.smallFont
                    color: Kirigami.Theme.disabledTextColor
                    wrapMode: Text.Wrap
                    Layout.fillWidth: true
                }
                QQC2.Label {
                    visible: AppSettings.ressacDbSet
                    text: AppSettings.ressacDbNote
                    font: Kirigami.Theme.smallFont
                    color: Kirigami.Theme.disabledTextColor
                    wrapMode: Text.Wrap
                    Layout.fillWidth: true
                }
                RowLayout {
                    visible: AppSettings.databaseRefusal !== ""
                    spacing: Kirigami.Units.smallSpacing
                    Kirigami.Icon { source: "dialog-warning"; implicitWidth: Kirigami.Units.iconSizes.small; implicitHeight: Kirigami.Units.iconSizes.small }
                    QQC2.Label {
                        text: AppSettings.databaseRefusal
                        color: Kirigami.Theme.negativeTextColor
                        wrapMode: Text.Wrap
                        Layout.fillWidth: true
                    }
                }
                RowLayout {
                    visible: AppSettings.needsRelaunch
                    spacing: Kirigami.Units.largeSpacing
                    QQC2.Label { text: AppSettings.relaunchNote; wrapMode: Text.Wrap; Layout.fillWidth: true }
                    QQC2.Button { text: AppSettings.relaunchNow; onClicked: settings.relaunchNow() }
                }
            }

            QQC2.Button {
                Kirigami.FormData.label: "Logs:"
                text: "Show Logs"
                onClicked: settings.showLogs()
            }
        }
    }

    FileDialog {
        id: chooseDb
        title: AppSettings.chooseTitle
        nameFilters: [AppSettings.sqliteFilter]
        onAccepted: AppSettings.chooseDatabase(String(selectedFile))
    }

    FileDialog {
        id: newDb
        title: AppSettings.newTitle
        fileMode: FileDialog.SaveFile
        defaultSuffix: "sqlite"
        nameFilters: [AppSettings.sqliteFilter]
        onAccepted: AppSettings.chooseDatabase(String(selectedFile))
    }
}
