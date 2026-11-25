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
    // The pane on screen for the look. An inactive one (Native's, hidden under Antiope) keeps its
    // page and never navigates; it loads the current page when it becomes active again.
    property bool active: true
    // The fetch failure notice over the page; Antiope draws its own teletext row instead.
    property bool showsNotice: true
    // Antiope's pane: its pages hold mosaic boxes, marked here when their image failed.
    property bool mosaics: false

    function runAppScript(script, done) {
        web.runJavaScript(script, WebEngineScript.ApplicationWorld, done)
    }

    function runSubpageScript() {
        const script = Reader.subpageScript()
        if (script !== "")
            pane.runAppScript(script, function () {})
    }

    function focusWeb() {
        web.forceActiveFocus()
    }

    // Space: scroll the page; done(true) when the key is spent (nothing to scroll yet, or scrolled),
    // done(false) when the page is at its end or there is nothing to read.
    function pageDown(entryId, done) {
        if (entryId !== Reader.entryId) {          // the selected entry's page is not the one on screen yet
            done(true)
            return
        }
        if (!Reader.hasEntry) {
            done(false)
            return
        }
        pane.runAppScript(Reader.pageDownScript(String(web.url)), function (r) { done(r === "spent" || r === "scrolled") })
    }

    function openExternally(url) {
        if (Recette.active)
            console.warn("reader: open " + url)
        else
            Qt.openUrlExternally(url)
    }

    function copyText(text) {
        clipboard.text = text
        clipboard.selectAll()
        clipboard.copy()
    }

    // Walk hook: a synthetic click in the app world reaches navigationRequested as LinkClickedNavigation.
    function clickTitleLink() {
        pane.runAppScript('document.querySelector("h1.title a").click()', function () {})
    }

    // Walk hook: a click on the element `#<id>` in the app world (a mosaic box's toggle link). Only
    // a plain id is accepted, so nothing but a name ever reaches the script.
    function clickSelector(selector) {
        const id = /^#([A-Za-z][A-Za-z0-9_-]*)$/.exec(selector)
        if (!Recette.active || id === null)
            return
        pane.runAppScript('document.getElementById("' + id[1] + '").click()', function () {})
    }

    TextEdit { id: clipboard; visible: false }

    // A fetch that failed without a stored error (a stored one is already in the page).
    QQC2.Label {
        id: notice
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        height: visible ? implicitHeight : 0
        visible: pane.showsNotice && Reader.runFailureNotice !== ""
        text: Reader.runFailureNotice
        color: Kirigami.Theme.disabledTextColor
        wrapMode: Text.Wrap
        leftPadding: 16
        rightPadding: 16
        topPadding: 6
        bottomPadding: 6
        background: Rectangle { color: Kirigami.Theme.alternateBackgroundColor }
    }

    QQC2.Menu {
        id: menu
        property string selection: ""
        property string link: ""

        QQC2.MenuItem {
            text: "Copy"
            enabled: menu.selection !== ""
            onTriggered: web.triggerWebAction(WebEngineView.Copy)
        }
        QQC2.MenuItem {
            text: "Copy Link"
            visible: menu.link !== ""
            height: visible ? implicitHeight : 0
            onTriggered: pane.copyText(menu.link)
        }
    }

    WebEngineView {
        id: web
        anchors.top: notice.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        visible: Reader.hasEntry || pane.proofing
        backgroundColor: Reader.colorScheme === "dark" ? "#1C1C1E" : "#FFFFFF"

        settings.javascriptEnabled: false
        settings.javascriptCanOpenWindows: false
        settings.focusOnNavigationEnabled: false
        settings.playbackRequiresUserGesture: true
        settings.pluginsEnabled: false
        settings.localContentCanAccessFileUrls: false

        // Our own load is allowed; a clicked web link opens in the browser; everything else is refused.
        // Refused first, so a decision that never comes (an error) leaves the navigation refused.
        // ressac-toggle: is a registered scheme (shim.cpp), so a mosaic box's click arrives here
        // like any link, with or without a user gesture.
        onNavigationRequested: (request) => {
            request.reject()
            const d = Reader.decide(String(request.url), request.navigationType, request.isMainFrame, String(web.url))
            if (d === "allow") {
                request.accept()
                return
            }
            if (d.startsWith("open:"))
                pane.openExternally(d.slice(5))
            else if (d.startsWith("toggle:"))         // a mosaic box clicked: it swaps with its original
                pane.runAppScript(Reader.toggleImageScript(Number(d.slice(7))), function () {})
        }

        onNewWindowRequested: (request) => {
            const d = Reader.decide(String(request.requestedUrl), 0, true, "")
            if (d.startsWith("open:"))
                pane.openExternally(d.slice(5))
        }

        onContextMenuRequested: (request) => {
            request.accepted = true
            menu.selection = request.selectedText
            const d = Reader.decide(String(request.linkUrl), 0, true, "")
            menu.link = d.startsWith("open:") ? d.slice(5) : ""
            menu.popup()
        }

        onJavaScriptConsoleMessage: (level, message) => console.debug("reader page:", message)

        onLoadingChanged: (info) => {
            if (info.status !== WebEngineView.LoadSucceededStatus)
                return
            const proof = pane.proofing
            // A sub-page label that moved while the page loaded: the page shows the current one.
            if (!proof && pane.active)
                pane.runSubpageScript()
            if (!Recette.active)
                return
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

    // Only the active pane follows the reader's page.
    Binding {
        target: web
        property: "url"
        value: pane.proofing ? Reader.proofUrl : Reader.pageUrl
        when: pane.active
        restoreMode: Binding.RestoreNone
    }

    // Antiope's sub-page line ("412 3/20") changes in place, without a reload.
    Connections {
        target: Reader
        function onSubpageChanged() {
            if (pane.active && !pane.proofing)
                pane.runSubpageScript()
        }
    }

    // A mosaic answered with the failure placeholder: its boxes show the magenta IMAGE line. The
    // target also creates `Mosaics` with the first pane, before any article loads (without it,
    // every image would get the placeholder and no `failed`).
    Connections {
        target: Mosaics
        enabled: pane.mosaics && pane.active
        function onFailed(url) {
            if (Recette.active)
                console.warn("mosaic: failed " + url)
            pane.runAppScript(Reader.markFailedScript(url), function () {})
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
