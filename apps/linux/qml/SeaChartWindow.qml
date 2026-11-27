import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtWebEngine
import QtWebChannel
import org.kde.kirigami as Kirigami
import org.w3cdotorg.ressac

// The Sea Chart (spec §7.8): the Mac's unchanged page at ressac:chart/index.html in a WebEngineView of a
// window of its own. The page talks through window.webkit.messageHandlers.ressac, an adapter injected as a
// user script (with qwebchannel.js) that posts to `relay` over a WebChannel; the documents go in through
// Chart.load / Chart.update scripts run from the `Chart` singleton's `runScript` signal.
QQC2.ApplicationWindow {
    id: chartWindow

    property var chrome                          // Main.qml: chartClosed() drops this window

    title: Chart.windowTitle
    minimumWidth: 640
    minimumHeight: 360
    width: AppSettings.chartWindowWidth
    height: AppSettings.chartWindowHeight
    color: Kirigami.Theme.backgroundColor

    // Plasma's instant animations (longDuration 1 with the org.kde.desktop style) are the chart's Reduce Motion.
    Binding { target: Chart; property: "reduceMotion"; value: Kirigami.Units.longDuration <= 1 }
    // Not on screen: hidden or minimised (Wayland gives no occlusion).
    Binding { target: Live; property: "chartOnScreen"; value: chartWindow.visible && chartWindow.visibility !== Window.Minimized }
    Timer { interval: Chart.tickMs; repeat: true; running: Chart.open; onTriggered: Chart.tick() }

    onClosing: {
        AppSettings.chartWindowWidth = width
        AppSettings.chartWindowHeight = height
        Chart.closeWindow()
        Live.chartOnScreen = false
        Qt.callLater(chrome.chartClosed)
    }

    // A walk's draw probe: logs its facts (unless `tag` is empty) and hands them to `done`, when given. A
    // silent probe is a poll that may land before the page has a document: it answers "pending" then,
    // instead of letting the page log an error.
    function probe(tag, done) {
        const script = tag === "" ? "(function () { try { return " + Chart.probeScript + " } catch (e) { return 'colours=0 pending' } })()" : Chart.probeScript
        web.runJavaScript(script, function (result) {
            if (tag !== "") console.warn("walk: " + tag + " " + result)
            if (done) done(result)
        })
    }

    function runPage(script) { web.runJavaScript(script) }

    QtObject {
        id: relay
        WebChannel.id: "ressac"
        function post(text) { Chart.receive(String(text)) }
    }

    WebChannel { id: channel; registeredObjects: [relay] }

    // A still chart (Reduce Motion) loses its frame when the window is minimised (WebRedraw.qml).
    readonly property alias redraw: redraw
    readonly property real viewHeight: web.height        // the `redraw` walk: 1 px less during a redraw
    WebRedraw { id: redraw }

    WebEngineView {
        id: web
        anchors.fill: parent
        anchors.bottomMargin: redraw.inset
        visible: Launcher.phase === Launcher.Ready && Chart.phase === "ready"
        webChannel: channel
        settings.javascriptCanOpenWindows: false
        settings.localContentCanAccessRemoteUrls: false

        Component.onCompleted: {
            const qwebchannel = WebEngine.script()
            qwebchannel.name = "qwebchannel"
            qwebchannel.sourceUrl = "qrc:/qtwebchannel/qwebchannel.js"
            qwebchannel.injectionPoint = WebEngineScript.DocumentReady
            qwebchannel.worldId = WebEngineScript.MainWorld
            qwebchannel.runsOnSubFrames = false          // main frame only: a subframe has no way to post
            const adapter = WebEngine.script()
            adapter.name = "ressac-adapter"
            adapter.sourceCode = Chart.adapterScript
            adapter.injectionPoint = WebEngineScript.DocumentReady
            adapter.worldId = WebEngineScript.MainWorld
            adapter.runsOnSubFrames = false
            web.userScripts.insert([qwebchannel, adapter])
            web.url = Chart.pageUrl
        }

        onNavigationRequested: (request) => {
            if (Chart.allowsNavigation(String(request.url), request.isMainFrame))
                request.accept()
            else
                request.reject()
        }
        onNewWindowRequested: (request) => {}
        onLoadingChanged: (info) => {
            if (info.status === WebEngineView.LoadStartedStatus)
                Chart.pageLoadStarted()
            else
                Chart.pageLoaded(String(info.url), info.status === WebEngineView.LoadSucceededStatus)
        }
        onRenderProcessTerminated: web.reload()
        onJavaScriptConsoleMessage: (level, message, line, source) => {
            if (level === WebEngineView.ErrorMessageLevel)
                console.warn("chart page: " + message)
        }
    }

    // Undo and redo go to the page first: when it has nothing to undo (`Chart.undo()` is false),
    // the keystroke is the focused text field's own, as the Mac's menu does.
    Shortcut {
        sequences: [StandardKey.Undo]
        onActivated: web.runJavaScript("Chart.undo()", function (undone) { if (!undone) web.triggerWebAction(WebEngineView.Undo) })
    }
    Shortcut {
        sequences: ["Ctrl+Shift+Z"]
        onActivated: web.runJavaScript("Chart.redo()", function (redone) { if (!redone) web.triggerWebAction(WebEngineView.Redo) })
    }

    Connections {
        target: Chart
        function onRunScript(script) { web.runJavaScript(script) }
        function onGoHome() { web.url = Chart.pageUrl }
        function onPrefsReported(view, ink, animate) { AppSettings.saveChartPrefs(view, ink, animate, Chart.reduceMotion) }
    }

    // The launch finished (or was redone) with the window up: survey again against the new database.
    Connections {
        target: Launcher
        function onPhaseChanged() {
            if (Launcher.phase === Launcher.Ready && Chart.open) {
                Chart.closeWindow()
                Chart.openWindow()
            }
        }
    }

    QQC2.BusyIndicator {
        anchors.centerIn: parent
        running: visible
        visible: (Launcher.phase === Launcher.Ready && Chart.phase === "loading")
            || Launcher.phase === Launcher.Starting || Launcher.phase === Launcher.Migrating
    }

    QQC2.Label {
        anchors.centerIn: parent
        visible: Launcher.phase === Launcher.TooNew || Launcher.phase === Launcher.Failed
        text: Chart.notOpenText
        color: Kirigami.Theme.disabledTextColor
        padding: 40
    }

    ColumnLayout {
        anchors.centerIn: parent
        width: Math.min(parent.width - Kirigami.Units.gridUnit * 4, Kirigami.Units.gridUnit * 30)
        spacing: Kirigami.Units.largeSpacing
        visible: Launcher.phase === Launcher.Ready && Chart.phase === "failed"

        Kirigami.Heading {
            level: 3
            text: Chart.notDrawnText
            Layout.alignment: Qt.AlignHCenter
        }

        Kirigami.SelectableLabel {
            text: Chart.message
            color: Kirigami.Theme.disabledTextColor
            wrapMode: Text.Wrap
            horizontalAlignment: Text.AlignHCenter
            Layout.fillWidth: true
        }

        QQC2.Button {
            text: Chart.retryText
            Layout.alignment: Qt.AlignHCenter
            onClicked: Chart.retry()
        }
    }

    // A refresh that failed while the old chart stays up.
    Rectangle {
        visible: web.visible && Chart.refreshError !== ""
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 16
        width: Math.min(parent.width - 32, capsule.implicitWidth + Kirigami.Units.largeSpacing * 2)
        height: capsule.implicitHeight + Kirigami.Units.smallSpacing * 2
        radius: height / 2
        color: Qt.rgba(Kirigami.Theme.backgroundColor.r, Kirigami.Theme.backgroundColor.g, Kirigami.Theme.backgroundColor.b, 0.9)

        RowLayout {
            id: capsule
            anchors.centerIn: parent
            width: parent.width - Kirigami.Units.largeSpacing * 2
            spacing: Kirigami.Units.smallSpacing

            QQC2.Label {
                text: Chart.refreshError
                maximumLineCount: 2
                elide: Text.ElideRight
                textFormat: Text.PlainText
                Layout.fillWidth: true
            }

            QQC2.Button {
                text: Chart.retryText
                onClicked: Chart.retry()
            }
        }
    }
}
