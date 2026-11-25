pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls as QQC2
import org.w3cdotorg.ressac

// The Minitel bar at the foot of the Antiope screen: the five keys (SOMMAIRE, SUITE, RETOUR, GUIDE,
// RÉPÉTITION), each followed by one black cell, then the hint or status line, or the search prompt
// (` SEARCH? ` and a field bound to the search text) while it is open. A click on a key does what its
// keyboard key does: the verb goes through `readingPage.perform`, Antiope first then Native.
Row {
    id: bar

    property real size: 20
    readonly property real cellWidth: 0.6 * size
    // The cells right of the keys: the hint, or the prompt.
    property int cells: 0
    property bool promptOpen: false
    readonly property bool promptFocused: prompt.activeFocus

    // Return or a click elsewhere keep the search (`clearing` false); Esc clears it.
    signal closeRequested(bool clearing)

    height: size

    function focusPrompt() {
        prompt.forceActiveFocus()
    }

    // The page that answers the window's keys (objectName "readingPage"), else the Antiope view.
    function perform(verb) {
        let fallback = null
        for (let item = bar.parent; item; item = item.parent) {
            if (item.objectName === "readingPage")
                return item.perform(verb)
            if (fallback === null && item.objectName === "antiopeView")
                fallback = item
        }
        return fallback ? fallback.perform(verb) : false
    }

    Repeater {
        model: Antiope.keyLines.length

        Row {
            id: key

            required property int index

            CellRow {
                line: Antiope.keyLines[key.index]
                size: bar.size

                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: bar.perform(Antiope.keyVerbs[key.index])
                }
            }

            Item {
                width: bar.cellWidth
                height: bar.size
            }
        }
    }

    CellRow {
        objectName: "minitelHint"
        visible: !bar.promptOpen
        line: Antiope.minitel
        size: bar.size
    }

    CellRow {
        id: promptLabel
        visible: bar.promptOpen
        line: Antiope.searchLabel(bar.cells)
        size: bar.size
    }

    // Typing goes to the field: the window's key filter leaves every key to a text field, so digits
    // are text here, never a page number.
    QQC2.TextField {
        id: prompt
        objectName: "antiopePrompt"
        visible: bar.promptOpen
        width: Math.max(0, bar.cells * bar.cellWidth - promptLabel.width)
        height: bar.size
        padding: 0
        verticalAlignment: TextInput.AlignVCenter
        font.family: "Bedstead"
        font.pixelSize: bar.size
        color: "#FFFF00"
        selectionColor: "#FFFFFF"
        selectedTextColor: "#000000"
        placeholderText: "____"
        placeholderTextColor: "#FFFF00"
        background: Rectangle { color: "black" }
        text: EntriesModel.searchText
        onTextEdited: EntriesModel.searchText = text
        onAccepted: bar.closeRequested(false)
        Keys.onEscapePressed: bar.closeRequested(true)
        onActiveFocusChanged: if (!activeFocus) bar.closeRequested(false)
    }
}
