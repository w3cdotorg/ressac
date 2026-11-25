pragma ComponentBehavior: Bound
import QtQuick
import org.w3cdotorg.ressac

// A list of teletext rows (Antiope): one CellRow per encoded `Line` of `rows`, diffed by id in
// CellRowsModel so a change never resets the list. Keys are the window filter's, not the list's.
ListView {
    id: list

    property alias rows: rowsModel.rows
    property real size: 20
    // The CRT's colour bleed on every row.
    property bool bleed: false

    // A row with an action was clicked, `x` in the row's own pixels (a pair row's half).
    signal activated(string action, real x)

    // Keeps the selected row on screen.
    function showSelected() {
        const row = rowsModel.rowOfSelected()
        if (row >= 0)
            positionViewAtIndex(row, ListView.Contain)
    }

    clip: true
    boundsBehavior: Flickable.StopAtBounds
    keyNavigationEnabled: false
    activeFocusOnTab: false

    model: CellRowsModel { id: rowsModel }

    delegate: CellRow {
        id: row

        // `line` and `doubleHeight` are CellRow's own properties: the roles come in through
        // `model`, never as required properties of the same names.
        required property var model
        readonly property string action: model.action

        line: model.line
        size: list.size
        bleed: list.bleed
        doubleHeight: model.doubleHeight
        width: list.width
        height: (row.doubleHeight ? 2 : 1) * list.size

        MouseArea {
            anchors.fill: parent
            enabled: row.action !== ""
            cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
            onClicked: (mouse) => list.activated(row.action, mouse.x)
        }
    }

    onRowsChanged: showSelected()
}
