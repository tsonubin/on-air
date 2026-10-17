pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls

ListView {
    id: root
    required property var board
    required property string side
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    spacing: 0
    ScrollBar.vertical: ScrollBar {
        policy: ScrollBar.AsNeeded
        x: root.side === "input" ? 0 : root.width - width
        visible: root.contentHeight > root.height
        enabled: visible
    }

    function rowAt(point) {
        var children = contentItem.children;
        for (var i = 0; i < children.length; ++i) {
            var row = children[i];
            if (!row.nodeId)
                continue;
            var p = row.mapFromItem(root.board, point.x, point.y);
            var y = row.mapToItem(root, 0, 0).y;
            if (y + row.height > 0 && y < height && p.x >= 0 && p.x < row.width && p.y >= 0 && p.y < row.height)
                return row.nodeId;
        }
        return "";
    }
    function portFor(id) {
        var children = contentItem.children;
        for (var i = 0; i < children.length; ++i) {
            var row = children[i];
            if (row.nodeId === id) {
                var p = row.socket.mapToItem(root.board, row.socket.width / 2, row.socket.height / 2);
                var top = root.mapToItem(root.board, 0, 0).y;
                p.y = Math.max(top + 5, Math.min(top + height - 5, p.y));
                return p;
            }
        }
        return null;
    }
    onContentYChanged: root.board.refreshCables()
    onCountChanged: Qt.callLater(root.board.refreshCables)

    delegate: Item {
        id: row
        required property var modelData
        property var node: modelData.node
        property string nodeId: node.id
        property alias socket: socket
        width: root.width
        height: root.board.unit(29)
        onYChanged: Qt.callLater(root.board.refreshCables)
        Component.onCompleted: Qt.callLater(root.board.refreshCables)

        Rectangle {
            anchors.fill: parent
            anchors.margins: 1
            color: root.board.bodyDropId === row.nodeId ? Qt.alpha(root.board.accent, 0.22) : root.board.selectedId === row.nodeId ? Qt.alpha(root.board.foreground, 0.10) : body.containsMouse ? Qt.alpha(root.board.foreground, 0.05) : "transparent"
        }
        MouseArea {
            id: body
            anchors.fill: parent
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            hoverEnabled: true
            property point origin
            property bool dragging: false
            onPressed: function (mouse) {
                root.board.forceActiveFocus();
                root.board.interactionActive = true;
                origin = Qt.point(mouse.x, mouse.y);
                dragging = false;
                if (mouse.button === Qt.RightButton)
                    root.board.showMenu(row.node, row.mapToItem(root.board, mouse.x, mouse.y));
            }
            onPositionChanged: function (mouse) {
                if (!pressed || pressedButtons !== Qt.LeftButton)
                    return;
                if (!dragging && Math.abs(mouse.x - origin.x) + Math.abs(mouse.y - origin.y) > 8)
                    dragging = true;
                if (dragging)
                    root.board.moveBody(row.nodeId, row.mapToItem(root.board, mouse.x, mouse.y));
            }
            onReleased: function (mouse) {
                if (mouse.button !== Qt.LeftButton) {
                    root.board.endInteraction();
                    return;
                }
                if (dragging)
                    root.board.finishBody(row.nodeId);
                else
                    root.board.choose(row.nodeId);
                dragging = false;
                root.board.endInteraction();
            }
            onCanceled: {
                dragging = false;
                root.board.cancelDrag();
            }
        }
        ToolTip.visible: body.containsMouse && !body.pressed
        ToolTip.delay: 700
        ToolTip.text: node.name + (node.members.length ? " · " + node.members.length + " members" : "") + "\n" + node.detail

        Row {
            anchors.verticalCenter: parent.verticalCenter
            x: root.side === "input" ? root.board.unit(5 + (row.modelData.child ? 10 : 0)) : root.board.unit(23 + (row.modelData.child ? 10 : 0))
            spacing: root.board.unit(5)
            Text {
                width: root.board.unit(13)
                text: row.node.members.length ? (root.board.expanded[row.nodeId] ? "▾" : "▸") : root.board.glyph(row.node)
                color: root.board.foreground
                font.family: root.board.fontFamily
                font.pixelSize: root.board.fontPixels
                MouseArea {
                    anchors.fill: parent
                    enabled: row.node.members.length > 0
                    onClicked: root.board.toggleGroup(row.nodeId)
                }
            }
            Text {
                width: Math.max(0, row.width - parent.x - root.board.unit(root.side === "input" ? 50 : 40))
                text: row.node.name
                color: row.node.state === "unavailable" ? root.board.muted : root.board.foreground
                font.family: root.board.fontFamily
                font.pixelSize: root.board.fontPixels
                font.bold: row.node.is_default
                elide: Text.ElideRight
                font.strikeout: root.board.isMuted(row.node)
            }
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            x: root.side === "input" ? parent.width - root.board.unit(33) : parent.width - root.board.unit(18)
            text: row.node.is_default ? "✓" : row.node.members.length ? String(row.node.members.length) : (row.node.kind === "airplay" || row.node.kind === "sonos") ? "󰖩" : ""
            color: row.node.state === "failed" ? root.board.urgent : root.board.foreground
            font.family: root.board.fontFamily
            font.pixelSize: root.board.fontPixels
        }
        Item {
            id: socket
            width: root.board.unit(18)
            height: row.height
            x: root.side === "input" ? row.width - width : 0
            Rectangle {
                anchors.centerIn: parent
                width: root.board.unit(7)
                height: width
                radius: width / 2
                color: root.board.hasLink(row.nodeId) ? root.board.accent : "transparent"
                border.width: 1
                border.color: root.board.hasLink(row.nodeId) ? root.board.accent : root.board.muted
            }
            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.CrossCursor
                preventStealing: true
                onPressed: function (mouse) {
                    root.board.startCable(row.nodeId, socket.mapToItem(root.board, socket.width / 2, socket.height / 2));
                }
                onPositionChanged: function (mouse) {
                    if (pressed)
                        root.board.moveCable(socket.mapToItem(root.board, mouse.x, mouse.y));
                }
                onReleased: function (mouse) {
                    root.board.finishCable(row.nodeId, socket.mapToItem(root.board, mouse.x, mouse.y));
                }
                onCanceled: root.board.cancelDrag()
            }
        }
    }
}
