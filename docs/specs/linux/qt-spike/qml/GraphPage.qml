import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtWebEngine
import QtWebChannel
import org.kde.kirigami as Kirigami

Kirigami.Page {
    id: page
    property var backend
    title: "Carte (WebEngine)"
    padding: 0

    // Objects visible to page JS through qwebchannel.js
    QtObject {
        id: qmlBridge
        WebChannel.id: "qmlBridge"
        signal pinged(string from)
        function hello(name) { console.log("[spike] JS -> QML hello(" + name + ")"); return "QML says hi to " + name }
    }
    WebChannel { id: channel; registeredObjects: [qmlBridge, page.backend] }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0
        QQC2.Label {
            id: statusLabel
            Layout.fillWidth: true
            padding: Kirigami.Units.smallSpacing
            text: "backend.status = " + page.backend.status + "   |   runJavaScript: " + jsResult
            property string jsResult: "(pending)"
        }
        WebEngineView {
            id: web
            Layout.fillWidth: true
            Layout.fillHeight: true
            webChannel: channel
            url: Qt.resolvedUrl("../web/graph.html")
            onJavaScriptConsoleMessage: (level, message, line, source) => console.log("[web] " + message)
            onLoadingChanged: (info) => {
                console.log("[spike] web loading status=" + info.status + " " + info.url + " err=" + info.errorString)
                if (info.status === WebEngineView.LoadSucceededStatus)
                    clickTimer.start()
            }
            onRenderProcessTerminated: (status, code) => console.log("[spike] render process terminated " + status + " " + code)
        }
    }
    Timer {
        id: clickTimer
        interval: 2500
        onTriggered: web.runJavaScript("simulateClick('Rust Blog'); 'nodes=' + document.querySelectorAll('circle').length + ' ua=' + navigator.userAgent.split(' ').slice(-2).join(' ')",
                                       (r) => { statusLabel.jsResult = r; console.log("[spike] runJavaScript -> " + r) })
    }
}
