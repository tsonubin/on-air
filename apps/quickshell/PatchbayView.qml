pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls

Item {
    id: root
    property var snapshot: ({
            nodes: [],
            links: [],
            enabled: true,
            supported: true
        })
    property string clientError: ""
    property color foreground: "#a9b1d6"
    property color background: "#1a1b26"
    property color accent: "#7aa2f7"
    property color muted: "#565f89"
    property color urgent: "#f7768e"
    property string fontFamily: "monospace"
    property int fontPixels: 12
    property real uiScale: 1
    property bool reducedMotion: false
    property string selectedId: ""
    property var expanded: ({})
    property string bodyDropId: ""
    property string bodyDragId: ""
    property var pendingLink: null
    property var pendingVolume: null
    property var pendingMutes: ({})
    property var displayedNodes: []
    property bool interactionActive: false
    property bool ready: false
    property bool panelActive: true
    readonly property var inputRows: rows("input")
    readonly property var outputRows: rows("output")
    readonly property int rowCount: Math.max(2, Math.min(5, Math.max(inputRows.length, outputRows.length)))
    readonly property string message: clientError || snapshot.error || (!snapshot.enabled ? "Audio paused · click to resume" : "") || ((snapshot.links || []).length === 0 && !pendingLink && displayedNodes.length > 0 ? "No cables · drag from a socket to the other side to connect" : "")
    readonly property var selectedNode: node(selectedId) || (snapshot.nodes || []).find(function (n) {
        return n.side === "output" && n.is_default;
    }) || (snapshot.nodes || []).find(function (n) {
        return n.side === "output";
    }) || null
    signal command(var body)
    signal closeRequested
    signal resumeRequested
    implicitWidth: root.unit(380)
    implicitHeight: root.unit(24 + 18 + rowCount * 29 + 38 + (message ? 30 : 0))
    focus: true

    function unit(value) {
        return Math.round(value * uiScale);
    }
    function node(id) {
        return displayedNodes.find(function (n) {
            return n.id === id;
        }) || null;
    }
    function glyph(n) {
        if (n.kind === "monitor")
            return "󰍹";
        if (n.side === "input")
            return "󰍬";
        if (n.kind === "airplay")
            return "󰀂";
        if (/headphone|headset/i.test(n.name))
            return "󰋋";
        return "󰓃";
    }
    function isMuted(n) {
        return n && Object.prototype.hasOwnProperty.call(pendingMutes, n.id) ? pendingMutes[n.id] : n ? n.muted : false;
    }
    function toggleMute(id) {
        var n = node(id);
        if (!n)
            return;
        var next = Object.assign({}, pendingMutes);
        next[id] = !isMuted(n);
        pendingMutes = next;
        command({
            op: "set_muted",
            id: id,
            muted: next[id]
        });
    }
    function rows(side) {
        var nodes = displayedNodes, memberIds = {}, result = [];
        nodes.forEach(function (n) {
            if (n.side === side && n.kind === "group")
                n.members.forEach(function (id) {
                    memberIds[id] = true;
                });
        });
        var top = nodes.filter(function (n) {
            return n.side === side && (!memberIds[n.id] || n.kind === "group");
        });
        top.sort(function (a, b) {
            return Number(b.is_default) - Number(a.is_default) || Number(a.kind === "group") - Number(b.kind === "group") || a.name.localeCompare(b.name);
        });
        top.forEach(function (n) {
            result.push({
                node: n,
                child: false
            });
            if (expanded[n.id])
                n.members.forEach(function (id) {
                    var member = node(id);
                    if (member)
                        result.push({
                            node: member,
                            child: true
                        });
                });
        });
        return result;
    }
    function visualId(id) {
        var n = node(id);
        if (!n)
            return id;
        var group = displayedNodes.find(function (g) {
            return g.kind === "group" && g.side === n.side && g.members.indexOf(id) >= 0 && !expanded[g.id];
        });
        return group ? group.id : id;
    }
    function portFor(id) {
        var n = node(id);
        return n ? (n.side === "input" ? inputs : outputs).portFor(visualId(id)) : null;
    }
    function nodeAt(point, side) {
        return (side === "input" ? inputs : outputs).rowAt(point);
    }
    function hasLink(id) {
        return (snapshot.links || []).some(function (l) {
            return visualId(l.source) === id || visualId(l.target) === id;
        });
    }
    function refreshCables() {
        if (ready)
            Qt.callLater(cables.sync);
    }
    function choose(id) {
        selectedId = id;
        command({
            op: "set_default",
            id: id
        });
        forceActiveFocus();
    }
    function adoptSnapshot() {
        if (!ready || interactionActive)
            return;
        var nodes = snapshot.nodes || [];
        if (JSON.stringify(nodes) === JSON.stringify(displayedNodes))
            return;
        var inputY = inputs.contentY, outputY = outputs.contentY;
        displayedNodes = nodes;
        Qt.callLater(function () {
            inputs.contentY = Math.max(0, Math.min(inputY, inputs.contentHeight - inputs.height));
            outputs.contentY = Math.max(0, Math.min(outputY, outputs.contentHeight - outputs.height));
            refreshCables();
        });
    }
    function endInteraction() {
        interactionActive = false;
        Qt.callLater(adoptSnapshot);
    }
    function toggleGroup(id) {
        var next = Object.assign({}, expanded);
        next[id] = !next[id];
        expanded = next;
        refreshCables();
    }
    function moveBody(id, point) {
        bodyDragId = id;
        var source = node(id), target = source ? nodeAt(point, source.side) : "";
        bodyDropId = target !== id ? target : "";
    }
    function finishBody(id) {
        var target = bodyDropId;
        cancelDrag();
        if (target && node(id))
            command({
                op: "combine",
                side: node(id).side,
                members: [id, target]
            });
    }
    function startCable(id, point) {
        interactionActive = true;
        forceActiveFocus();
        cables.begin(id, point);
    }
    function moveCable(point) {
        cables.move(point);
    }
    function finishCable(id, point) {
        var source = node(id), targetId = source ? nodeAt(point, source.side === "input" ? "output" : "input") : "";
        if (!source || !targetId) {
            cables.finish(null);
            endInteraction();
            return;
        }
        var link = {
            source: source.side === "input" ? id : targetId,
            target: source.side === "output" ? id : targetId,
            state: "pending"
        };
        pendingLink = link;
        selectedId = link.target;
        cables.finish(link);
        command({
            op: "connect",
            source: link.source,
            target: link.target
        });
        endInteraction();
    }
    function cancelDrag() {
        bodyDropId = "";
        bodyDragId = "";
        cables.finish(null);
        endInteraction();
    }
    function requestDone() {
        pendingLink = null;
        pendingVolume = null;
        pendingMutes = ({});
        refreshCables();
    }
    function showMenu(n, point) {
        selectedId = n.id;
        actions.currentNode = n;
        actions.x = point.x;
        actions.y = point.y;
        actions.popup();
    }
    function disconnectSelected() {
        var id = selectedId;
        (snapshot.links || []).filter(function (l) {
            return l.source === id || l.target === id;
        }).forEach(function (l) {
            command({
                op: "disconnect",
                source: l.source,
                target: l.target
            });
        });
    }
    onSnapshotChanged: {
        adoptSnapshot();
        refreshCables();
    }
    Component.onCompleted: {
        ready = true;
        adoptSnapshot();
    }
    onPanelActiveChanged: if (!panelActive)
        cancelDrag()
    onVisibleChanged: if (!visible)
        cancelDrag()
    onWidthChanged: refreshCables()
    Keys.onPressed: function (event) {
        if (event.key === Qt.Key_Escape) {
            if (cables.dragRope || bodyDragId)
                cancelDrag();
            else
                closeRequested();
            event.accepted = true;
            return;
        }
        var current = node(selectedId) || selectedNode;
        if (!current)
            return;
        var list = current.side === "input" ? inputRows : outputRows;
        var index = list.findIndex(function (r) {
            return r.node.id === current.id;
        });
        if (event.key === Qt.Key_Up || event.key === Qt.Key_Down) {
            let next = Math.max(0, Math.min(list.length - 1, index + (event.key === Qt.Key_Up ? -1 : 1)));
            if (list[next])
                selectedId = list[next].node.id;
        } else if (event.key === Qt.Key_Left || event.key === Qt.Key_Right) {
            let opposite = event.key === Qt.Key_Left ? inputRows : outputRows;
            if (opposite.length)
                selectedId = opposite[Math.min(Math.max(index, 0), opposite.length - 1)].node.id;
        } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter)
            choose(current.id);
        else if (event.key === Qt.Key_Space || event.key === Qt.Key_M)
            toggleMute(current.id);
        else if (event.key === Qt.Key_Delete || event.key === Qt.Key_Backspace)
            disconnectSelected();
        else {
            event.accepted = false;
            return;
        }
        event.accepted = true;
    }

    Text {
        x: root.unit(12)
        y: root.unit(10)
        text: "Input"
        color: root.foreground
        font.family: root.fontFamily
        font.pixelSize: root.fontPixels
    }
    Text {
        x: outputs.x + root.unit(18)
        y: root.unit(10)
        text: "Output"
        color: root.foreground
        font.family: root.fontFamily
        font.pixelSize: root.fontPixels
    }
    NodeColumn {
        id: inputs
        x: root.unit(8)
        y: root.unit(32)
        width: (root.width - root.unit(72)) / 2
        height: root.rowCount * root.unit(29)
        board: root
        side: "input"
        model: root.inputRows
    }
    NodeColumn {
        id: outputs
        x: root.width - root.unit(8) - width
        y: inputs.y
        width: inputs.width
        height: inputs.height
        board: root
        side: "output"
        model: root.outputRows
    }
    CableLayer {
        id: cables
        anchors.fill: parent
        links: root.pendingLink ? (root.snapshot.links || []).concat([root.pendingLink]) : root.snapshot.links || []
        resolvePort: root.portFor
        accent: root.accent
        muted: root.muted
        urgent: root.urgent
        reducedMotion: root.reducedMotion
        animatingAllowed: root.panelActive
        cableBounds: Qt.rect(inputs.x + inputs.width - root.unit(9), inputs.y, outputs.x + root.unit(9) - (inputs.x + inputs.width - root.unit(9)), inputs.height)
    }
    MouseArea {
        // Covers the whole drawn cable, sockets included, so any visible
        // stretch of rope can be right-clicked. A click that misses every
        // rope falls through to the row menu when a row is under it.
        x: inputs.x + inputs.width - root.unit(18)
        y: inputs.y - root.unit(12)
        width: outputs.x + root.unit(18) - x
        height: inputs.height + root.unit(24)
        acceptedButtons: Qt.RightButton
        onClicked: function (mouse) {
            var point = Qt.point(x + mouse.x, y + mouse.y);
            var link = cables.linkAt(point);
            if (link) {
                root.command({
                    op: "disconnect",
                    source: link.source,
                    target: link.target
                });
                return;
            }
            var id = root.nodeAt(point, "input") || root.nodeAt(point, "output");
            if (id)
                root.showMenu(root.node(id), point);
        }
    }
    Text {
        visible: root.inputRows.length === 0 && root.outputRows.length === 0
        anchors.centerIn: inputs
        text: root.clientError ? "Offline" : "Connecting…"
        color: root.muted
        font.family: root.fontFamily
        font.pixelSize: root.fontPixels
    }
    Text {
        x: root.unit(12)
        y: inputs.y + inputs.height + root.unit(3)
        width: root.width - root.unit(24)
        height: root.message ? root.unit(30) : 0
        visible: root.message.length > 0
        text: root.message
        color: root.clientError || root.snapshot.error ? root.urgent : root.muted
        font.family: root.fontFamily
        font.pixelSize: root.fontPixels - 1
        elide: Text.ElideRight
        maximumLineCount: 2
        wrapMode: Text.Wrap
        MouseArea {
            anchors.fill: parent
            enabled: !root.snapshot.enabled
            onClicked: root.resumeRequested()
        }
    }
    Rectangle {
        x: root.unit(12)
        y: root.height - root.unit(36)
        width: root.width - root.unit(24)
        height: 1
        color: Qt.alpha(root.foreground, 0.16)
    }
    Text {
        x: root.unit(14)
        y: root.height - root.unit(28)
        width: root.unit(24)
        height: root.unit(20)
        text: root.isMuted(root.selectedNode) ? "󰝟" : "󰕾"
        color: root.foreground
        font.family: root.fontFamily
        font.pixelSize: root.fontPixels + 2
        MouseArea {
            anchors.fill: parent
            enabled: root.selectedNode !== null
            onClicked: root.toggleMute(root.selectedNode.id)
        }
    }
    Slider {
        id: volume
        objectName: "volumeSlider"
        x: root.unit(44)
        y: root.height - root.unit(32)
        width: root.width - root.unit(100)
        height: root.unit(28)
        from: 0
        to: 100
        stepSize: 1
        enabled: root.selectedNode !== null && root.snapshot.enabled
        value: root.pendingVolume && root.selectedNode && root.pendingVolume.id === root.selectedNode.id ? root.pendingVolume.volume : root.selectedNode ? root.selectedNode.volume : 0
        onMoved: {
            if (root.selectedNode) {
                root.pendingVolume = {
                    id: root.selectedNode.id,
                    volume: Math.round(value)
                };
                volumeDebounce.restart();
            }
        }
        onPressedChanged: if (!pressed && root.pendingVolume) {
            volumeDebounce.stop();
            root.command({
                op: "set_volume",
                id: root.pendingVolume.id,
                volume: root.pendingVolume.volume
            });
        }
        background: Rectangle {
            x: volume.leftPadding
            y: volume.topPadding + volume.availableHeight / 2 - height / 2
            width: volume.availableWidth
            height: root.unit(3)
            radius: height / 2
            color: Qt.alpha(root.foreground, 0.22)
            Rectangle {
                width: volume.visualPosition * parent.width
                height: parent.height
                radius: parent.radius
                color: root.accent
            }
        }
        handle: Rectangle {
            x: volume.leftPadding + volume.visualPosition * (volume.availableWidth - width)
            y: volume.topPadding + volume.availableHeight / 2 - height / 2
            width: root.unit(10)
            height: width
            radius: width / 2
            color: root.accent
        }
        ToolTip.visible: hovered
        ToolTip.text: root.selectedNode ? root.selectedNode.name : "Volume"
    }
    Text {
        x: root.width - root.unit(48)
        y: root.height - root.unit(26)
        text: Math.round(volume.value) + "%"
        color: root.foreground
        font.family: root.fontFamily
        font.pixelSize: root.fontPixels
    }
    Timer {
        id: volumeDebounce
        interval: 120
        onTriggered: if (root.pendingVolume)
            root.command({
                op: "set_volume",
                id: root.pendingVolume.id,
                volume: root.pendingVolume.volume
            })
    }

    Menu {
        id: actions
        font.family: root.fontFamily
        font.pixelSize: root.fontPixels
        palette.window: root.background
        palette.base: root.background
        palette.text: root.foreground
        palette.windowText: root.foreground
        palette.buttonText: root.foreground
        palette.highlight: root.accent
        palette.highlightedText: root.background
        property var currentNode: null
        MenuItem {
            text: "Set as system default"
            onTriggered: if (actions.currentNode)
                root.choose(actions.currentNode.id)
        }
        MenuItem {
            text: root.isMuted(actions.currentNode) ? "Unmute" : "Mute"
            onTriggered: if (actions.currentNode)
                root.toggleMute(actions.currentNode.id)
        }
        MenuItem {
            text: "Disconnect routes"
            enabled: actions.currentNode && root.hasLink(actions.currentNode.id)
            onTriggered: root.disconnectSelected()
        }
        MenuItem {
            text: "Remove group"
            visible: actions.currentNode !== null && actions.currentNode.kind === "group"
            height: visible ? implicitHeight : 0
            onTriggered: root.command({
                op: "remove_group",
                id: actions.currentNode.id
            })
        }
    }
}
