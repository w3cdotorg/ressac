pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import org.w3cdotorg.ressac

// The Antiope look (spec §7.9): a black teletext screen of whole cells. The header row; three columns
// (the page index, the entries, the reader) with a blue rule between them, each under its title row;
// the Minitel bar at the foot, the run banner under the header when there is one. The rules are the
// `Antiope` singleton's; this view binds its inputs, draws its rows and applies the effects of what it
// answers (`apply`) and of the rows clicked (`activate`). Focus follows `Antiope.column`.
FocusScope {
    id: view

    objectName: "antiopeView"

    readonly property real size: AppSettings.readerSizeAntiope
    // The CRT screen (the view exists only in Antiope): scanlines, vignette, corners, colour bleed.
    readonly property bool crtOn: AppSettings.crt
    readonly property real cellWidth: 0.6 * size
    readonly property bool reduceMotion: Kirigami.Units.longDuration <= 1
    readonly property int windowCells: Math.floor(width / cellWidth)
    // The index, the entries and the two rules: the reader has the rest.
    readonly property int readerCells: windowCells - Antiope.indexCells - Antiope.entryCells - 2
    readonly property Item reader: antiopeReader
    readonly property alias entriesList: entriesList
    // The entries column's visible list: the entry list, or the garden's or P104's rows. Both are
    // named `antiopeEntries` (the key filter's focus chain), so a lookup takes this one.
    readonly property Item entriesItem: entriesPage === "list" ? entriesList : entriesCells
    // The entries column: the garden's feeds (P800–P802), P104's edition, or the entry list.
    readonly property string entriesPage: SourcesModel.isGarden ? "garden"
        : SourcesModel.selectedKey === "journal" ? "journal" : "list"
    // The reader column, in the Mac's order: P100, P899 or the end page; the inspector in a garden;
    // NO ENTRY SELECTED; else the article.
    readonly property string readerShows: Antiope.readerPage !== "article" ? "rows"
        : SourcesModel.isGarden ? "inspector"
        : EntriesModel.selectedEntryId < 0 ? "rows" : "article"

    // The search prompt in the Minitel bar (Ctrl+F). The view goes with the look, so the prompt
    // never survives a switch; the search text does.
    property bool promptOpen: false
    readonly property bool promptFocused: minitelBar.promptFocused
    // Set by `leave()`: the look is switching away, nothing may take the focus back.
    property bool leaving: false

    // One page-down at a time; a page that never answers must not leave Antiope advancing for good.
    property int advanceGeneration: 0

    // The recette's scroll measurement (GridProbe.measure's loop on the entries list).
    property int probeFramesLeft: 0
    property real probeStep: 0
    property var probeDone: null

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
                antiopeReader.runAppScript(Reader.toggleAllScript(), function () {})
                break
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
            if (view.entriesPage === "list")
                entriesList.forceActiveFocus()
            else
                entriesCells.forceActiveFocus()
            break
        case "reader":
            if (view.readerShows !== "article")
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

    // Walk hook: scrolls the entries `entriesPerFrame` two-row entries per frame for `frames` frames,
    // the frame times logged; `done` gets `Recette.frameLogStop()`.
    function scrollProbe(entriesPerFrame, frames, done) {
        entriesList.positionViewAtBeginning()
        probeStep = entriesPerFrame * 2 * view.size
        probeFramesLeft = frames
        probeDone = done
        Recette.frameLogStart()
        entriesList.contentY = entriesList.originY + 1     // the first move asks for the first frame
    }

    function clickEntry(row) {
        EntriesModel.select(row)
        Antiope.showArticle()
        Antiope.column = "entries"
        focusColumn()
    }

    function openPrompt() {
        promptOpen = true
        minitelBar.focusPrompt()
    }

    // `clearing` empties the search (Esc); `refocus` false when the view is being left.
    function closePrompt(clearing, refocus) {
        if (!promptOpen)
            return
        promptOpen = false
        if (clearing)
            EntriesModel.searchText = ""
        if (refocus !== false)
            focusColumn()
    }

    // The look switches away (the view is about to go): an open prompt closes keeping its text, and
    // an open repairs question is dropped so `Garden.confirming` never stays true with no dialog.
    function leave() {
        leaving = true
        closePrompt(false, false)
        if (Garden.confirming && repairs.visible)
            Garden.confirming = false                    // its `stateChanged` closes the dialog
    }

    function openLog() {
        if (Recette.active)
            console.warn("walk: open log")
        else
            Qt.openUrlExternally("file://" + encodeURI(Runs.logDir))
    }

    // A click on a row with an action (`Line::action`), `x` in the row's pixels.
    function activate(action, x) {
        const parts = action.split(":")
        switch (parts[0]) {
        case "jump":                                     // an index row, a page of P100's
            apply(Antiope.jump(Number(parts[1])))
            break
        case "pair": {                                   // P100's two pages on one row: the half clicked
            const page = x < Number(parts[3]) * view.cellWidth ? parts[1] : parts[2]
            if (page !== "-")
                apply(Antiope.jump(Number(page)))
            break
        }
        case "entry":                                    // an entry of P104's edition
            EntriesModel.selectEntry(Number(parts[1]))
            Antiope.showArticle()
            Antiope.column = "entries"
            focusColumn()
            break
        case "overflow":                                 // P104's "more from this feed": its page, or the feed
            if (parts[2] !== "-") {
                apply(Antiope.jump(Number(parts[2])))
            } else {
                SourcesModel.selectKey("feed:" + parts[1])
                Antiope.showArticle()
                Antiope.column = "index"
                focusColumn()
            }
            break
        case "feed":                                     // a feed of a garden page
            Garden.selectFeed(Number(parts[1]))
            Antiope.column = "entries"
            focusColumn()
            break
        case "cadence":                                  // the inspector's Rhythm and Theme choices
            Garden.setCadence(Number(parts[1]))
            Antiope.column = "reader"
            focusColumn()
            break
        case "theme":
            Garden.setTheme(Number(parts[1]))
            Antiope.column = "reader"
            focusColumn()
            break
        }
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
    Binding { target: Antiope; property: "journalFailure"; value: Journal.failure }
    Binding { target: Antiope; property: "journalDone"; value: Journal.progressDone }
    Binding { target: Antiope; property: "journalTotal"; value: Journal.progressTotal }
    Binding { target: Antiope; property: "windowCells"; value: view.windowCells }
    Binding { target: Antiope; property: "readerCells"; value: view.readerCells }
    Binding { target: Antiope; property: "windowWidth"; value: view.Window.window ? view.Window.window.width : 0 }
    Binding { target: Antiope; property: "windowHeight"; value: view.Window.window ? view.Window.window.height : 0 }
    Binding { target: Antiope; property: "cellWidth"; value: view.cellWidth }
    Binding { target: Antiope; property: "crt"; value: view.crtOn }
    // The reader page's bleed (`data-crt`); the binding goes with the view, so Native never shows it.
    Binding { target: Reader; property: "crt"; value: view.crtOn }
    // The garden inspector's page, as wide as the reader column.
    Binding { target: Garden; property: "antiopeCells"; value: view.readerCells }

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
            bleed: view.crtOn
        }

        // The run banner: the message, ` LOG ` when the log explains it, ` X ` to dismiss.
        Row {
            id: banner
            objectName: "antiopeBanner"

            readonly property var lines: Runs.bannerVisible
                ? Antiope.bannerRow(Runs.bannerMessage, Runs.bannerShowLog, view.windowCells) : ["", "", ""]

            visible: Runs.bannerVisible
            height: view.size

            CellRow {
                line: banner.lines[0]
                size: view.size
                bleed: view.crtOn
            }

            CellRow {
                visible: banner.lines[1] !== ""
                line: banner.lines[1]
                size: view.size
                bleed: view.crtOn

                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: view.openLog()
                }
            }

            CellRow {
                line: banner.lines[2]
                size: view.size
                bleed: view.crtOn

                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: Runs.dismissBanner()
                }
            }
        }

        Row {
            width: parent.width
            // The header, the banner when shown, and the Minitel bar.
            height: Math.max(0, view.height - (banner.visible ? 3 : 2) * view.size)

            FocusScope {
                id: indexColumn

                width: Antiope.indexCells * view.cellWidth
                height: parent.height

                onActiveFocusChanged: if (activeFocus) Antiope.column = "index"

                CellRow {
                    id: indexTitle
                    line: Antiope.indexTitle
                    size: view.size
                    bleed: view.crtOn
                }

                CellList {
                    id: indexList
                    objectName: "antiopeIndex"
                    anchors.top: indexTitle.bottom
                    anchors.bottom: parent.bottom
                    width: parent.width
                    size: view.size
                    bleed: view.crtOn
                    rows: Antiope.indexRows
                    onActivated: (action, x) => view.activate(action, x)
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
                    bleed: view.crtOn

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
                    visible: view.entriesPage === "list"
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
                        bleed: view.crtOn
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
                    visible: view.entriesPage === "list" && entriesList.count === 0
                    line: Antiope.emptyLine(EntriesModel.emptyText)
                    size: view.size
                    bleed: view.crtOn
                }

                // P800–P802 (the garden's feeds; its keys, ↑ and ↓ move between them) or P104's edition.
                CellList {
                    id: entriesCells
                    objectName: "antiopeEntries"
                    anchors.top: entriesTitle.bottom
                    anchors.bottom: parent.bottom
                    width: parent.width
                    size: view.size
                    bleed: view.crtOn
                    visible: view.entriesPage !== "list"
                    rows: view.entriesPage === "garden" ? Garden.antiopeRows
                        : view.entriesPage === "journal" ? Antiope.journalRows : []
                    onActivated: (action, x) => view.activate(action, x)
                    Keys.onPressed: (event) => { event.accepted = view.entriesPage === "garden" && Garden.key(event.text, event.modifiers) }
                    Keys.onUpPressed: if (view.entriesPage === "garden") Garden.antiopeMove(-1)
                    Keys.onDownPressed: if (view.entriesPage === "garden") Garden.antiopeMove(1)

                    // P104's list follows the selection (its rows move after the selection signal).
                    Connections {
                        target: EntriesModel
                        enabled: view.entriesPage === "journal"
                        function onSelectionChanged() { Qt.callLater(entriesCells.showSelected) }
                    }
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
                    bleed: view.crtOn
                }

                // A fetch that failed without a stored error, black on red over the article.
                CellRow {
                    id: failureNotice
                    objectName: "antiopeNotice"
                    anchors.top: readerTitle.bottom
                    visible: view.readerShows === "article" && Reader.runFailureNotice !== ""
                    height: visible ? view.size : 0
                    line: visible ? Antiope.failureNotice(Reader.runFailureNotice, view.readerCells) : ""
                    size: view.size
                    bleed: view.crtOn
                }

                ReaderPane {
                    id: antiopeReader
                    objectName: "readerPane"
                    proofing: false                         // the recette proves the native pane's page once
                    showsNotice: false                      // the teletext row above says it
                    mosaics: true
                    anchors.top: failureNotice.bottom
                    anchors.bottom: parent.bottom
                    width: parent.width
                    visible: view.readerShows === "article"
                }

                // P100, P899, the end of the edition, NO ENTRY SELECTED, or the garden's inspector.
                CellList {
                    id: readerList
                    objectName: "antiopeReader"
                    anchors.top: readerTitle.bottom
                    anchors.bottom: parent.bottom
                    width: parent.width
                    size: view.size
                    bleed: view.crtOn
                    visible: view.readerShows !== "article"
                    rows: view.readerShows === "inspector" ? Garden.antiopeInspector
                        : view.readerShows === "rows" ? Antiope.readerRows : []
                    onActivated: (action, x) => view.activate(action, x)
                }
            }
        }

        MinitelBar {
            id: minitelBar
            size: view.size
            bleed: view.crtOn
            cells: view.windowCells - Antiope.keysCells
            promptOpen: view.promptOpen
            onCloseRequested: (clearing) => view.closePrompt(clearing)
        }
    }

    // The CRT screen over everything, last in the view; it takes no input.
    CrtOverlay {
        anchors.fill: parent
        z: 1000
        visible: view.crtOn
        textSize: view.size
    }

    // The garden's Undo (Ctrl+Z), Native's being hidden with its pane.
    QQC2.Action {
        shortcut: StandardKey.Undo
        enabled: SourcesModel.isGarden
        onTriggered: Garden.undo()
    }

    // Shift+R in a garden: every safe repair, after this question (B5's texts and buttons).
    Kirigami.PromptDialog {
        id: repairs
        title: Garden.applyQuestion
        subtitle: Garden.applyPreview
        standardButtons: QQC2.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action {
                id: confirmAction
                text: Garden.applyConfirmLabel
                onTriggered: { Garden.applySafe(); repairs.close() }
            },
            Kirigami.Action {
                text: "Cancel"
                onTriggered: repairs.close()
            }
        ]
        // As Native's (GardenPane): the keyboard on the confirm button.
        onOpened: {
            const button = repairs.customFooterButton(confirmAction)
            if (button)
                button.forceActiveFocus()
        }
        onClosed: {
            Garden.confirming = false
            if (!view.leaving)
                view.focusColumn()
        }
    }

    Connections {
        target: Garden
        function onStateChanged() {
            if (Garden.confirming && !repairs.visible)
                repairs.open()
            else if (!Garden.confirming && repairs.visible)
                repairs.close()
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

    Connections {
        target: view.Window.window
        enabled: view.probeFramesLeft > 0
        function onFrameSwapped() {
            if (view.probeFramesLeft <= 0)
                return
            view.probeFramesLeft -= 1
            if (view.probeFramesLeft === 0) {
                const done = view.probeDone
                view.probeDone = null
                done(Recette.frameLogStop())
                return
            }
            const top = entriesList.originY
            const next = entriesList.contentY + view.probeStep
            entriesList.contentY = next > top + entriesList.contentHeight - entriesList.height ? top : next
        }
    }

    Component.onCompleted: Antiope.reload()
    // Native came back during a page-down: Antiope must not stay advancing for good; nor may the
    // garden stay confirming when the question goes with the view (`leave()` first did both).
    Component.onDestruction: {
        if (advanceGuard.running)
            Antiope.advanceFinished()
        if (Garden.confirming && repairs.visible)
            Garden.confirming = false
        // A destroyed Binding leaves its target as it was: Native never shows the CRT.
        Reader.crt = false
        Antiope.crt = false
    }
}
