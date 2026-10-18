import QtQuick

// Now-playing surface for the Omarchy bar: the CD transport state and the
// active output, read from the core's HTTP API (no MPRIS bridge exists yet;
// see docs/omarchy.md "Media"). Shipped beside the plugin and wired as a
// custom QML bar module by packaging/omarchy/snippets/shell-bar.json.
// Collapses to nothing while the core is unreachable or nothing is playing,
// so a machine without a disc or an active route shows no phantom player.
//
// Interactions mirror Omarchy's own media widget: left = play/pause,
// middle = next track, scroll = previous/next, right = open on-air.
Item {
    id: root
    property int apiPort: 47990
    property int pollInterval: 5000
    // Custom QML bar module contract: the bar fills these after loading.
    property var bar: null
    property string moduleName: "on-air-media"
    property var settings: ({})
    property string fixedEdge: "start"

    readonly property bool vertical: bar ? !!bar.vertical : false
    readonly property var cd: client.error ? null : client.cd
    readonly property var output: client.error ? null : client.activeOutput
    readonly property bool quiet: !cd || !cd.present ? !output : false
    readonly property string glyph: cd && cd.present ? (cd.playing ? "󰐊" : "󰏤") : "󰝚"
    readonly property string line: {
        var parts = [];
        if (cd && cd.present) {
            parts.push(cd.track + "/" + cd.track_count);
            if (cd.title)
                parts.push(cd.title);
        }
        if (output)
            parts.push("→ " + output.device_name);
        return parts.join(" · ");
    }
    readonly property string detail: (cd && cd.present ? "CD " + line : line) + (cd && cd.present && !cd.playing ? " · paused" : "")

    implicitWidth: quiet ? 0 : vertical ? (bar && bar.barSize ? bar.barSize : 28) : content.implicitWidth + 12
    implicitHeight: bar && bar.barSize ? bar.barSize : 24
    visible: !quiet

    function run(command) {
        if (bar && bar.run)
            bar.run(command);
    }

    CoreClient {
        id: client
        port: root.apiPort
        idleInterval: root.pollInterval
    }

    Row {
        id: content
        anchors.verticalCenter: parent.verticalCenter
        x: 6
        spacing: 5
        Text {
            text: root.glyph
            color: root.bar ? root.bar.foreground : "#a9b1d6"
            font.family: root.bar ? root.bar.fontFamily : "monospace"
            font.pixelSize: 12
        }
        Text {
            visible: !root.vertical
            width: Math.min(implicitWidth, 180)
            text: root.line
            color: root.bar ? root.bar.foreground : "#a9b1d6"
            font.family: root.bar ? root.bar.fontFamily : "monospace"
            font.pixelSize: 12
            elide: Text.ElideRight
        }
    }

    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton
        hoverEnabled: true
        onEntered: if (root.bar && root.bar.showTooltip)
            root.bar.showTooltip(root.moduleName, root.detail)
        onExited: if (root.bar && root.bar.hideTooltip)
            root.bar.hideTooltip(root.moduleName)
        onClicked: function (mouse) {
            if (mouse.button === Qt.RightButton || !root.cd || !root.cd.present) {
                root.run("omarchy-launch-or-focus-tui on-air");
                return;
            }
            if (mouse.button === Qt.MiddleButton)
                client.cdControl("next");
            else if (mouse.button === Qt.LeftButton)
                client.cdControl(root.cd.playing ? "pause" : "play");
        }
    }
    WheelHandler {
        onWheel: function (event) {
            if (root.cd && root.cd.present)
                client.cdControl(event.angleDelta.y < 0 ? "next" : "prev");
        }
    }
}
