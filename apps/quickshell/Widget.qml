import QtQuick
import Quickshell
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
        onUpdated: if (!busy && queue.length === 0)
            board.requestDone()
        onToast: function (title, body, urgency) {
            // Freedesktop notifications: Omarchy's shell daemon shows these
            // as toasts (mako on Omarchy 3). Quiet when notify-send is
            // absent; nothing else depends on it.
            Quickshell.execDetached(["notify-send", "-a", "on-air", "-u", urgency, title, body]);
        }
    }
    readonly property string mediaLine: {
        var parts = [];
        var cd = client.cd;
        if (cd && cd.present)
            parts.push((cd.playing ? "󰐊 " : "󰏤 ") + (cd.title ? cd.title : "CD " + cd.track + "/" + cd.track_count));
        var output = client.activeOutput;
        if (output)
            parts.push("→ " + output.device_name);
        return parts.join(" · ");
    }
    Ui.BarIconButton {
        id: button
        anchors.fill: parent
        bar: root.bar
        text: "󰝚"
        slotSize: Style.bar.statusSlot
        tooltipText: client.error || (root.mediaLine ? "on-air · " + root.mediaLine : "on-air patchbay · click to open")
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
        contentHeight: panel.fittedContentHeight(board.implicitHeight, Style.space(320))
        focusTarget: board
        // Omarchy 4's KeyboardPanel takes its content as default children.
        PatchbayView {
            id: board
            anchors.fill: parent
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
