import QtQuick
import QtQuick.Window
import org.w3cdotorg.ressac

// The CRT screen over the Antiope view (spec §7.9): a dark band on the lower part of every font-pixel
// row (the bands come from `Antiope.scanlineBands`, the Rust geometry; never computed here), a
// vignette, and the rounded corners of the tube. Drawn once, and again only when the size, the text
// size or the device pixel ratio changes: scrolling never repaints it. It takes no input.
Item {
    id: overlay

    // The view's text size, in pixels: a font pixel is a tenth of it.
    property real textSize: 20
    // How many times the canvas has painted (the recette reads it: 1 after a long scroll).
    property int paints: 0

    enabled: false

    Canvas {
        id: canvas

        anchors.fill: parent
        renderTarget: Canvas.Image
        renderStrategy: Canvas.Immediate

        onWidthChanged: requestPaint()
        onHeightChanged: requestPaint()

        Connections {
            target: overlay
            function onTextSizeChanged() { canvas.requestPaint() }
        }

        Connections {
            target: Screen
            function onDevicePixelRatioChanged() { canvas.requestPaint() }
        }

        onPaint: {
            const ctx = getContext("2d")
            const w = width
            const h = height
            ctx.reset()
            ctx.clearRect(0, 0, w, h)
            if (w <= 0 || h <= 0)
                return
            overlay.paints += 1

            // The scanlines: flattened (top, bottom) pairs.
            const bands = Antiope.scanlineBands(h, overlay.textSize / 10, Screen.devicePixelRatio)
            ctx.fillStyle = "rgba(0, 0, 0, 0.28)"
            for (let i = 0; i + 1 < bands.length; i += 2)
                ctx.fillRect(0, bands[i], w, bands[i + 1] - bands[i])

            // The vignette: clear at the centre, 0.35 black at half the diagonal.
            const radius = Math.sqrt(w * w + h * h) / 2
            const gradient = ctx.createRadialGradient(w / 2, h / 2, 0, w / 2, h / 2, radius)
            gradient.addColorStop(0, "rgba(0, 0, 0, 0)")
            gradient.addColorStop(1, "rgba(0, 0, 0, 0.35)")
            ctx.fillStyle = gradient
            ctx.fillRect(0, 0, w, h)

            // The tube's corners: the window's rectangle minus its rounded one, in black (the
            // fill-rule property; `fill("evenodd")` paints everything black).
            const r = Math.min(w, h) * 3 / 100
            ctx.fillStyle = "black"
            ctx.beginPath()
            ctx.rect(0, 0, w, h)
            ctx.roundedRect(0, 0, w, h, r, r)
            ctx.fillRule = Qt.OddEvenFill
            ctx.fill()
        }
    }
}
