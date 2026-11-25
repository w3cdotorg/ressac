pragma ComponentBehavior: Bound
import QtQuick
import org.w3cdotorg.ressac

// The Minitel bar at the foot of the Antiope screen: the five keys (SOMMAIRE, SUITE, RETOUR, GUIDE,
// RÉPÉTITION), each followed by one black cell, then the hint or status line. A click on a key does
// what its keyboard key does: the verb goes through `readingPage.perform`, Antiope first then Native.
Row {
    id: bar

    property real size: 20
    readonly property real cellWidth: 0.6 * size

    height: size

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
        line: Antiope.minitel
        size: bar.size
    }
}
