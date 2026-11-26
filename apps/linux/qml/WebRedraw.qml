import QtQuick

// Keeps a WebEngineView drawn across a stop of its window's scene graph (recette fix 1). Qt Quick
// stops the scene graph when the window is no longer exposed (minimised, or under a locked
// session: the threaded render loop emits sceneGraphAboutToStop), and Qt WebEngine 6.11 then frees
// the texture of the view's last frame (RenderWidgetHostViewQtDelegateItem::releaseResources). Once
// the window draws again the view shows black until Chromium sends a new frame, which a still page
// never does by itself. So, at the first frame after a stop, `inset` is 1 for one frame: the view,
// whose anchors.bottomMargin is bound to it, is 1 px shorter, and Chromium draws again at the new
// size, then at the old one.
Item {
    id: redraw

    // 1 for the frame of a redraw, else 0: the view's anchors.bottomMargin.
    property int inset: 0
    // The scene graph stopped since the last redraw.
    property bool stopped: false
    // Redraws done (the `redraw` walk reads it).
    property int redraws: 0

    // Emitted on the render thread; QML runs the handler on the window's thread.
    Connections {
        target: redraw.Window.window
        function onSceneGraphAboutToStop() { redraw.stopped = true }
    }

    Connections {
        target: redraw.Window.window
        enabled: redraw.stopped || redraw.inset > 0
        function onFrameSwapped() {
            if (redraw.inset > 0) {
                redraw.inset = 0
                redraw.redraws += 1
            } else {
                redraw.stopped = false
                redraw.inset = 1
            }
        }
    }
}
