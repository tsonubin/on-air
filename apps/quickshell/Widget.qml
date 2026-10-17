import QtQuick
import qs.Ui as Ui
import qs.Commons
import qs.Commons as Commons

Ui.Panel {
    id: root
    moduleName: "on-air.patchbay"
    ipcTarget: "on-air.patchbay"
    implicitWidth: button.implicitWidth
    implicitHeight: button.implicitHeight

    CoreClient {
        id: client
        panelOpen: root.opened
        port: Number(root.setting("port", 47990))
        onUpdated: if (panel.loadedContent && !busy && queue.length === 0)
            panel.loadedContent.requestDone()
    }
    Ui.BarIconButton {
        id: button
        anchors.fill: parent
        bar: root.bar
        text: "󰝚"
        slotSize: Style.bar.statusSlot
        tooltipText: client.error || "on-air"
        onPressed: function (mouseButton) {
            if (mouseButton === Qt.LeftButton || mouseButton === Qt.MiddleButton)
                root.toggle();
        }
    }
    Ui.KeyboardPanel {
        id: panel
        anchorItem: button
        owner: root
        bar: root.bar
        open: root.opened
        padding: 0
        contentWidth: panel.fittedContentWidth(Style.space(380))
        contentHeight: panel.fittedContentHeight(panel.loadedContentHeight, Style.space(320))
        focusTarget: panel.loadedContent
        content: Component {
            PatchbayView {
                panelActive: root.opened
                snapshot: client.snapshot
                clientError: client.error
                foreground: Commons.Color.popups.text
                background: Commons.Color.popups.background
                accent: Commons.Color.accent
                muted: Commons.Color.muted
                urgent: Commons.Color.urgent
                fontFamily: Style.font.family
                fontPixels: Style.font.body
                uiScale: Style.spaceReal(100) / 100
                reducedMotion: Style.reduceMotion || root.setting("reducedMotion", false)
                onCommand: function (body) {
                    client.command(body);
                }
                onCloseRequested: root.close()
                onResumeRequested: client.command({
                    op: "__service",
                    enabled: true
                })
            }
        }
    }
}
