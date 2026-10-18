// Run with Qt's qml tool, or tests/render.py. The panel itself is ordinary
// Qt Quick and can be exercised without a compositor or Quickshell host.
import QtQuick
import QtQuick.Window

Window {
    id: window
    visible: true
    width: board.implicitWidth
    height: board.implicitHeight
    color: "#1a1b26"
    title: "on-air patchbay preview"
    property int apiPort: 47990
    property string previewFontFamily: "monospace"
    // Toasts the core client emitted, for the offscreen test harness.
    property var toastLog: []
    CoreClient {
        id: client
        objectName: "coreClient"
        port: window.apiPort
        panelOpen: true
        onUpdated: if (!busy && queue.length === 0)
            board.requestDone()
        onToast: function (title, body, urgency) {
            window.toastLog = window.toastLog.concat([body]);
        }
    }
    PatchbayView {
        id: board
        panelActive: window.visible
        objectName: "patchbay"
        anchors.fill: parent
        snapshot: client.snapshot
        clientError: client.error
        fontFamily: window.previewFontFamily
        onCommand: function (body) {
            client.command(body);
        }
        onResumeRequested: client.command({
            op: "__service",
            enabled: true
        })
        onCloseRequested: window.close()
    }
}
