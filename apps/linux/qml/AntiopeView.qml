pragma ComponentBehavior: Bound
import QtQuick
import org.kde.kirigami as Kirigami
import org.w3cdotorg.ressac

// The Antiope look (spec §7.9): a black teletext screen of whole cells. The header row; three columns
// (the page index, the entries, the reader) with a blue rule between them, each under its title row;
// the Minitel bar at the foot. The rules are the `Antiope` singleton's; this view binds its inputs,
// draws its rows and applies the effects of what it answers (`apply`). Focus follows `Antiope.column`.
FocusScope {
    id: view

    objectName: "antiopeView"

    readonly property real size: AppSettings.readerSizeAntiope
    readonly property real cellWidth: 0.6 * size
    readonly property bool reduceMotion: Kirigami.Units.longDuration <= 1
    readonly property int windowCells: Math.floor(width / cellWidth)
    // The index, the entries and the two rules: the reader has the rest.
    readonly property int readerCells: windowCells - Antiope.indexCells - Antiope.entryCells - 2
    readonly property Item reader: antiopeReader
    readonly property alias entriesList: entriesList

    // One page-down at a time; a page that never answers must not leave Antiope advancing for good.
    property int advanceGeneration: 0

    // A verb of the window's keys: Antiope's answer applied; false hands the key to the Native handling.
    function perform(verb) {
        return apply(Antiope.perform(verb))
    }

    // `Outcome::encode()`: "" (not handled) or "handled" then ";effect[:argument]" in order.
    function apply(encoded) {
        if (encoded === "")
            return false
        const parts = encoded.split(";")
        for (let i = 1; i < parts.length; i++) {
            const colon = parts[i].indexOf(":")
            const name = colon < 0 ? parts[i] : parts[i].slice(0, colon)
            const argument = colon < 0 ? "" : parts[i].slice(colon + 1)
            switch (name) {
            case "select":
                SourcesModel.selectKey(argument)
                break
            case "retry":
                Journal.retry()
                break
            case "refresh":
                Runs.refreshSource(SourcesModel.selectedKey)
                break
            case "images":
                break                                    // the mosaics come in Task 17
            case "focusEntries":
                if (!SourcesModel.isGarden)
                    EntriesModel.selectFirstIfNone()
                break
            case "markRead":
                if (EntriesModel.selectedEntryId === Number(argument) && !EntriesModel.selectedRead)
                    EntriesModel.toggleRead(EntriesModel.selectedRow)
                break
            case "pageDownThenEnd":
                pageDownThenEnd(Number(argument))
                break
            case "notFound":
                notFoundTimer.restart()
                break
            }
        }
        focusColumn()
        return true
    }

    // Space past P104's last entry: scroll, and at the bottom land on FIN DE L'EDITION. Antiope is told
    // the page-down is over on every path (spent or not, or never answered).
    function pageDownThenEnd(id) {
        const generation = ++advanceGeneration
        advanceGuard.restart()
        antiopeReader.pageDown(id, function (spent) {
            if (generation !== view.advanceGeneration)
                return
            advanceGuard.stop()
            if (!spent && EntriesModel.selectedEntryId === id && Antiope.readerPage === "article")
                view.apply(Antiope.landOnEnd())
            Antiope.advanceFinished()
        })
    }

    // The active focus on the item of `Antiope.column`.
    function focusColumn() {
        switch (Antiope.column) {
        case "entries":
            entriesList.forceActiveFocus()
            break
        case "reader":
            if (Antiope.readerPage !== "article")
                readerList.forceActiveFocus()
            else if (Reader.hasEntry)
                antiopeReader.focusWeb()
            else
                antiopeReader.forceActiveFocus()
            break
        default:
            indexList.forceActiveFocus()
        }
    }

    function clickEntry(row) {
        EntriesModel.select(row)
        Antiope.showArticle()
        Antiope.column = "entries"
        focusColumn()
    }

    function clickIndexRow(action) {
        if (action.startsWith("jump:"))
            apply(Antiope.jump(Number(action.slice(5))))
    }

    // One cell: black, with a blue rule half a cell wide on its right.
    component Rule: Item {
        width: view.cellWidth
        height: parent ? parent.height : 0

        Rectangle {
            anchors.right: parent.right
            width: view.cellWidth / 2
            height: parent.height
            color: Antiope.palette[4]
        }
    }

    // Antiope's inputs.
    Binding { target: Antiope; property: "selectionKey"; value: SourcesModel.selectedKey }
    Binding { target: Antiope; property: "searchText"; value: EntriesModel.searchText }
    Binding { target: Antiope; property: "isGarden"; value: SourcesModel.isGarden }
    Binding { target: Antiope; property: "unreadOnly"; value: EntriesModel.unreadOnly }
    Binding { target: Antiope; property: "selectedEntryId"; value: EntriesModel.selectedEntryId }
    Binding { target: Antiope; property: "selectedRead"; value: EntriesModel.selectedRead }
    Binding { target: Antiope; property: "selectedFeedId"; value: EntriesModel.selectedFeedId }
    Binding { target: Antiope; property: "entryPosition"; value: EntriesModel.selectedRow + 1 }
    Binding { target: Antiope; property: "entryTotal"; value: EntriesModel.total }
    Binding { target: Antiope; property: "canRefresh"; value: Runs.canRefresh }
    Binding { target: Antiope; property: "canNavigate"; value: EntriesModel.canNavigate }
    Binding { target: Antiope; property: "runActive"; value: Runs.longRunActive }
    Binding { target: Antiope; property: "runDone"; value: Runs.progressDone }
    Binding { target: Antiope; property: "runTotal"; value: Runs.progressTotal }
    Binding { target: Antiope; property: "reduceMotion"; value: view.reduceMotion }
    Binding { target: Antiope; property: "gardenMessage"; value: Garden.message }
    Binding { target: Antiope; property: "journalPhase"; value: Journal.phase }
    Binding { target: Antiope; property: "windowCells"; value: view.windowCells }
    Binding { target: Antiope; property: "readerCells"; value: view.readerCells }
    Binding { target: Antiope; property: "windowWidth"; value: view.Window.window ? view.Window.window.width : 0 }
    Binding { target: Antiope; property: "windowHeight"; value: view.Window.window ? view.Window.window.height : 0 }
    Binding { target: Antiope; property: "cellWidth"; value: view.cellWidth }
    Binding { target: Antiope; property: "crt"; value: false }      // B9 binds the CRT screen

    // The reader's sub-page line ("412 3/20").
    Binding { target: Reader; property: "subpagePage"; value: Antiope.subpagePage }
    Binding { target: Reader; property: "subpageIndex"; value: Antiope.subpageIndex }
    Binding { target: Reader; property: "subpageCount"; value: Antiope.subpageCount }

    Rectangle {
        anchors.fill: parent
        color: Antiope.palette[0]
    }

    Column {
        anchors.fill: parent

        CellRow {
            objectName: "antiopeHeader"
            line: Antiope.header
            size: view.size
        }

        Row {
            width: parent.width
            height: Math.max(0, view.height - 2 * view.size)     // the header and the Minitel bar

            FocusScope {
                id: indexColumn

                width: Antiope.indexCells * view.cellWidth
                height: parent.height

                onActiveFocusChanged: if (activeFocus) Antiope.column = "index"

                CellRow {
                    id: indexTitle
                    line: Antiope.indexTitle
                    size: view.size
                }

                CellList {
                    id: indexList
                    objectName: "antiopeIndex"
                    anchors.top: indexTitle.bottom
                    anchors.bottom: parent.bottom
                    width: parent.width
                    size: view.size
                    rows: Antiope.indexRows
                    onActivated: (action, x) => view.clickIndexRow(action)
                }
            }

            Rule {}

            FocusScope {
                id: entriesColumn

                width: Antiope.entryCells * view.cellWidth
                height: parent.height

                onActiveFocusChanged: if (activeFocus) Antiope.column = "entries"

                CellRow {
                    id: entriesTitle
                    line: Antiope.entriesTitle
                    size: view.size

                    MouseArea {
                        anchors.fill: parent
                        onClicked: {
                            Antiope.column = "entries"
                            if (!SourcesModel.isGarden)
                                EntriesModel.unreadOnly = !EntriesModel.unreadOnly
                            view.focusColumn()
                        }
                    }
                }

                ListView {
                    id: entriesList
                    objectName: "antiopeEntries"
                    anchors.top: entriesTitle.bottom
                    anchors.bottom: parent.bottom
                    width: parent.width
                    clip: true
                    boundsBehavior: Flickable.StopAtBounds
                    keyNavigationEnabled: false
                    activeFocusOnTab: false
                    model: EntriesModel

                    delegate: CellRow {
                        id: entryRow

                        required property int index
                        required property var model

                        line: model.antiopeLines
                        size: view.size
                        width: entriesList.width
                        reversed: index === EntriesModel.selectedRow
                        reverseForeground: 0
                        reverseBackground: 7

                        Component.onCompleted: EntriesModel.rowAppeared(index)

                        MouseArea {
                            anchors.fill: parent
                            onClicked: view.clickEntry(entryRow.index)
                        }
                    }

                    Connections {
                        target: EntriesModel
                        function onSelectionChanged() {
                            if (EntriesModel.selectedRow >= 0)
                                entriesList.positionViewAtIndex(EntriesModel.selectedRow, ListView.Contain)
                        }
                    }
                }

                CellRow {
                    objectName: "antiopeEmpty"
                    anchors.top: entriesTitle.bottom
                    visible: entriesList.count === 0
                    line: Antiope.emptyLine(EntriesModel.emptyText)
                    size: view.size
                }
            }

            Rule {}

            FocusScope {
                id: readerColumn

                width: Math.max(0, view.readerCells) * view.cellWidth
                height: parent.height

                onActiveFocusChanged: if (activeFocus) Antiope.column = "reader"

                CellRow {
                    id: readerTitle
                    line: Antiope.readerTitle
                    size: view.size
                }

                ReaderPane {
                    id: antiopeReader
                    objectName: "readerPane"
                    anchors.top: readerTitle.bottom
                    anchors.bottom: parent.bottom
                    width: parent.width
                    visible: Antiope.readerPage === "article"
                }

                // The reader's teletext pages (P100, P899, the end of the edition) come in Task 12.
                CellList {
                    id: readerList
                    objectName: "antiopeReader"
                    anchors.top: readerTitle.bottom
                    anchors.bottom: parent.bottom
                    width: parent.width
                    size: view.size
                    visible: Antiope.readerPage !== "article"
                }
            }
        }

        MinitelBar {
            size: view.size
        }
    }

    Timer {
        id: notFoundTimer
        interval: 2000                                   // NOT_FOUND_FOR
        onTriggered: Antiope.tick()
    }

    Timer {
        id: advanceGuard
        interval: 3000
        onTriggered: {
            view.advanceGeneration++
            Antiope.advanceFinished()
        }
    }

    // The clock: every second, every tenth while a long run rolls the page counter.
    Timer {
        interval: Antiope.runActive && !view.reduceMotion ? 100 : 1000
        repeat: true
        running: true
        onTriggered: Antiope.tick()
    }

    Component.onCompleted: Antiope.reload()
}
