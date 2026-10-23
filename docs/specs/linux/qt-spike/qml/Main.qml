import QtQuick
import QtQuick.Controls as QQC2
import QtWebChannel
import org.kde.kirigami as Kirigami
import org.ressac.spike

Kirigami.ApplicationWindow {
    id: root
    width: 1100
    height: 720
    visible: true
    title: "Ressac Qt spike"

    Backend { id: backend; status: "idle"; WebChannel.id: "backend" }
    FeedModel { id: feedModel }

    globalDrawer: Kirigami.GlobalDrawer {
        title: "Ressac"
        titleIcon: "application-rss+xml"
        isMenu: false
        modal: false
        width: 220
        actions: [
            Kirigami.Action { text: "Sources"; icon.name: "feed-subscribe"; onTriggered: root.pageStack.currentIndex = 0 },
            Kirigami.Action { text: "Carte"; icon.name: "map-flat"; onTriggered: root.showGraph() }
        ]
    }

    function showGraph() {
        if (pageStack.depth < 2)
            pageStack.push(Qt.resolvedUrl("GraphPage.qml"), { backend: backend })
    }

    pageStack.initialPage: FeedsPage { model: feedModel; backend: backend }

    Timer { interval: 300; running: true; onTriggered: feedModel.loadDemo() }
    Timer { interval: 3000; running: true; onTriggered: root.showGraph() }
}
