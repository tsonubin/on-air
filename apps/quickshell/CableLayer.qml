import QtQuick
import "Rope.js" as Rope

Canvas {
    id: root
    objectName: "cableLayer"
    property var links: []
    property var resolvePort
    property color accent: "#7aa2f7"
    property color muted: "#565f89"
    property color urgent: "#f7768e"
    property bool reducedMotion: false
    property bool animatingAllowed: true
    property var ropes: ({})
    property var dragRope: null
    property string dragId: ""
    property bool activeMotion: false
    property real previousTime: 0
    property rect cableBounds: Qt.rect(0, 0, width, height)
    // Generous enough to click the visible curve without aiming.
    property real hitTolerance: 9

    function bounds() {
        return {
            left: cableBounds.x,
            right: cableBounds.x + cableBounds.width,
            top: cableBounds.y,
            bottom: cableBounds.y + cableBounds.height
        };
    }
    function key(link) {
        return link.source + "\n" + link.target;
    }
    function sync() {
        if (!resolvePort)
            return;
        var next = {};
        var moving = false;
        for (var i = 0; i < links.length; ++i) {
            var link = links[i], a = resolvePort(link.source), b = resolvePort(link.target);
            var id = key(link);
            if (!a || !b) {
                if (ropes[id]) {
                    ropes[id].hidden = true;
                    next[id] = ropes[id];
                }
                continue;
            }
            var rope = ropes[id] || Rope.create(a, b);
            rope.hidden = false;
            Rope.anchor(rope, a, b);
            rope.state = link.state;
            rope.link = link;
            if (reducedMotion)
                Rope.settle(rope, bounds());
            moving = moving || rope.awake;
            next[id] = rope;
        }
        ropes = next;
        activeMotion = moving || dragRope !== null;
        requestPaint();
    }
    function begin(id, point) {
        dragId = id;
        dragRope = Rope.create(point, point);
        activeMotion = true;
        requestPaint();
    }
    function move(point) {
        if (!dragRope)
            return;
        var start = resolvePort(dragId);
        if (start)
            Rope.anchor(dragRope, start, point);
        if (reducedMotion)
            Rope.settle(dragRope, {
                left: 0,
                right: width,
                top: 0,
                bottom: height
            });
        activeMotion = true;
        requestPaint();
    }
    function finish(link) {
        if (link && dragRope) {
            if (dragId !== link.source)
                dragRope.points.reverse();
            var a = resolvePort(link.source), b = resolvePort(link.target);
            if (a && b) {
                Rope.anchor(dragRope, a, b);
                ropes[key(link)] = dragRope;
            }
        }
        dragRope = null;
        dragId = "";
        sync();
    }
    function linkAt(point) {
        var ids = Object.keys(ropes);
        for (var i = ids.length - 1; i >= 0; --i) {
            var rope = ropes[ids[i]];
            if (rope.hidden)
                continue;
            if (Rope.hit(rope, point, hitTolerance))
                return rope.link;
        }
        return null;
    }
    onLinksChanged: Qt.callLater(sync)
    onWidthChanged: Qt.callLater(sync)
    onHeightChanged: Qt.callLater(sync)
    onReducedMotionChanged: sync()

    Timer {
        interval: 16
        running: root.visible && root.animatingAllowed && !root.reducedMotion && root.activeMotion
        repeat: true
        onRunningChanged: root.previousTime = Date.now()
        onTriggered: {
            var now = Date.now(), dt = (now - root.previousTime) / 1000;
            root.previousTime = now;
            var awake = false, ids = Object.keys(root.ropes);
            for (var i = 0; i < ids.length; ++i)
                awake = Rope.step(root.ropes[ids[i]], dt, root.bounds()) || awake;
            if (root.dragRope && !root.reducedMotion) {
                Rope.step(root.dragRope, dt, {
                    left: 0,
                    right: root.width,
                    top: 0,
                    bottom: root.height
                });
                awake = true;
            }
            root.activeMotion = awake;
            root.requestPaint();
        }
    }
    onPaint: {
        var ctx = getContext("2d");
        ctx.clearRect(0, 0, width, height);
        function draw(rope, color, dashed) {
            var ps = rope.points;
            ctx.strokeStyle = color;
            ctx.lineWidth = 1.6;
            ctx.lineCap = "round";
            ctx.lineJoin = "round";
            ctx.setLineDash(dashed ? [3, 3] : []);
            ctx.beginPath();
            ctx.moveTo(ps[0].x, ps[0].y);
            for (var i = 1; i < ps.length - 1; ++i)
                ctx.quadraticCurveTo(ps[i].x, ps[i].y, (ps[i].x + ps[i + 1].x) / 2, (ps[i].y + ps[i + 1].y) / 2);
            ctx.lineTo(ps[ps.length - 1].x, ps[ps.length - 1].y);
            ctx.stroke();
        }
        var ids = Object.keys(ropes);
        for (var i = 0; i < ids.length; ++i) {
            var rope = ropes[ids[i]], failed = rope.state === "failed" || rope.state === "unavailable";
            if (!rope.hidden)
                draw(rope, failed ? urgent : rope.state === "paused" ? muted : accent, rope.state !== "connected");
        }
        if (dragRope)
            draw(dragRope, accent, false);
    }
}
