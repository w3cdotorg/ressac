import QtQuick
import org.w3cdotorg.ressac

// The Antiope grid measurement (spec §7.9): one list of two-row teletext entries, drawn either by
// CellRow (`painted`) or by one QML Text per run (`text`), scrolled while the frame times are logged.
Rectangle {
    id: probe
    color: "black"

    property string variant: "painted"
    property real size: Recette.gridSize
    property bool patternShown: false
    readonly property real cellWidth: size * 6 / 10
    readonly property var colors: ["#000000", "#ff0000", "#00ff00", "#ffff00", "#0000ff", "#ff00ff", "#00ffff", "#ffffff"]
    readonly property string runSeparator: "\u001f"
    readonly property string lineSeparator: "\u001e"
    property var patternLines: []

    function entryLine(index) { return (patternShown ? patternLines[index] : Recette.gridLine(index)) || "" }

    // The runs of row `row` of entry `index`: { x, fg, bg, text }, from the same encoded string CellRow decodes.
    function runs(index, row) {
        const rows = entryLine(index).split(lineSeparator)
        const out = []
        let cells = 0
        for (const run of (rows[row] || "").split(runSeparator)) {
            if (run.length < 2) continue
            const text = run.substring(2)
            out.push({ x: cells * cellWidth, fg: run[0] === "?" ? 5 : Number(run[0]), bg: Number(run[1]), text: text })
            cells += text.length
        }
        return out
    }

    function pattern() {
        const sep = runSeparator
        const row = (fg, bg, text) => fg + "" + bg + text
        let checks = "", blocks = []
        for (let i = 0; i < 28; i++) {
            checks += i % 2 === 0 ? "█" : " "
            blocks.push(row((i % 7) + 1, 0, "█"))
        }
        patternLines = [
            row(7, 0, checks) + lineSeparator + blocks.join(sep),
            row(7, 0, "ABCDEFGHIJKLMNOPQRSTUVWXYZ01") + lineSeparator + row(3, 4, "m".repeat(28))
        ]
        patternShown = true
        list.model = 2
        list.contentY = 0
    }

    // `frames` swaps with the list moving `entriesPerFrame` entries each time; `done` gets the log line.
    property int framesLeft: 0
    property int stepEntries: 1
    property var finished: null
    function measure(entriesPerFrame, frames, done) {
        patternShown = false
        list.model = 2000
        list.contentY = 0
        stepEntries = entriesPerFrame
        framesLeft = frames
        finished = done
        Recette.frameLogStart()
        list.contentY = 1                 // the first move asks for the first frame
    }
    Connections {
        target: probe.Window.window
        function onFrameSwapped() {
            if (probe.framesLeft <= 0) return
            probe.framesLeft -= 1
            if (probe.framesLeft === 0) {
                const done = probe.finished
                probe.finished = null
                done(Recette.frameLogStop())
                return
            }
            const next = list.contentY + probe.stepEntries * 2 * probe.size
            list.contentY = next > list.contentHeight - list.height ? 0 : next
        }
    }

    Component {
        id: paintedDelegate
        CellRow {
            required property int index
            line: probe.entryLine(index)
            size: probe.size
            width: 28 * probe.cellWidth
            height: 2 * probe.size
        }
    }
    Component {
        id: textDelegate
        Item {
            id: entry
            required property int index
            width: 28 * probe.cellWidth
            height: 2 * probe.size
            Repeater {
                model: 2
                Item {
                    id: rowItem
                    required property int index
                    y: index * probe.size
                    width: entry.width
                    height: probe.size
                    Repeater {
                        model: probe.runs(entry.index, rowItem.index)
                        Item {
                            required property var modelData
                            x: modelData.x
                            Rectangle {
                                visible: modelData.bg !== 0
                                width: modelData.text.length * probe.cellWidth
                                height: probe.size
                                color: probe.colors[modelData.bg]
                            }
                            Text {
                                text: modelData.text
                                color: probe.colors[modelData.fg]
                                renderType: Text.NativeRendering
                                antialiasing: false
                                font.family: "Bedstead"
                                font.pixelSize: probe.size
                                font.hintingPreference: Font.PreferFullHinting
                                font.kerning: false
                                font.preferShaping: false
                                height: probe.size
                                verticalAlignment: Text.AlignVCenter
                            }
                        }
                    }
                }
            }
        }
    }

    ListView {
        id: list
        objectName: "probeList"
        x: 23 * probe.cellWidth
        y: probe.size
        width: 28 * probe.cellWidth
        height: probe.height - probe.size
        clip: true
        interactive: false
        model: 2000
        reuseItems: true
        delegate: probe.variant === "text" ? textDelegate : paintedDelegate
    }
}
