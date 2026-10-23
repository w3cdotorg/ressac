import QtQuick
import QtQuick.Controls as QQC2
import QtWebEngine
import org.kde.kirigami as Kirigami
import org.w3cdotorg.ressac

// The article page (spec §7.4): ressac:page/<entry> in a WebEngineView with JavaScript off, so no
// article script ever runs; the app's own scripts run in WebEngineScript.ApplicationWorld.
Item {
    id: pane

    objectName: "readerPane"

    // The recette first loads the proof page (every kind of article script, no sanitiser, no CSP).
    property bool proofing: Recette.active

    function runAppScript(script, done) {
        web.runJavaScript(script, WebEngineScript.ApplicationWorld, done)
    }

    function focusWeb() {
        web.forceActiveFocus()
    }

    WebEngineView {
        id: web
        anchors.fill: parent
        visible: Reader.hasEntry || pane.proofing
        url: pane.proofing ? Reader.proofUrl : Reader.pageUrl
        backgroundColor: Reader.colorScheme === "dark" ? "#1C1C1E" : "#FFFFFF"

        settings.javascriptEnabled: false
        settings.javascriptCanOpenWindows: false
        settings.focusOnNavigationEnabled: false
        settings.playbackRequiresUserGesture: true
        settings.pluginsEnabled: false
        settings.localContentCanAccessFileUrls: false

        onNavigationRequested: (request) => (String(request.url) === String(web.url)
            && request.navigationType === WebEngineNavigationRequest.TypedNavigation && request.isMainFrame)
            ? request.accept() : request.reject()

        onJavaScriptConsoleMessage: (level, message) => console.debug("reader page:", message)

        onLoadingChanged: (info) => {
            if (info.status !== WebEngineView.LoadSucceededStatus || !Recette.active)
                return
            const proof = pane.proofing
            pane.runAppScript(Reader.probeScript(), function (result) {
                if (proof) {
                    console.warn("reader-proof: " + result)
                    pane.proofing = false
                } else {
                    console.warn("reader: " + info.url + " " + result)
                }
            })
        }
    }

    Kirigami.PlaceholderMessage {
        anchors.centerIn: parent
        visible: !Reader.hasEntry && !pane.proofing
        icon.name: "document-preview"
        text: "No Entry Selected"
        explanation: "Choose an entry in the list to read it here."
    }
}
