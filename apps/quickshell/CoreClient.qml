import QtQuick

// One serial write queue; an old poll can never overwrite a newer command.
Item {
    id: root
    property int port: 47990
    property bool panelOpen: false
    property var snapshot: ({
            nodes: [],
            links: [],
            enabled: true,
            supported: true
        })
    property string error: ""
    property bool busy: false
    property var queue: []
    property int generation: 0
    property int appliedGeneration: 0
    property var inFlight: []
    signal updated

    function refresh() {
        if (!busy)
            request("GET", null);
    }

    function command(body) {
        // Coalesce slider writes for the same node while a request runs.
        var pending = queue.slice();
        if (body.op === "set_volume")
            pending = pending.filter(function (item) {
                return item.op !== body.op || item.id !== body.id;
            });
        pending.push(body);
        queue = pending;
        drain();
    }

    function drain() {
        if (busy || queue.length === 0)
            return;
        var pending = queue.slice();
        var body = pending.shift();
        queue = pending;
        request(body.op === "__service" ? "PUT" : "POST", body.op === "__service" ? {
            enabled: body.enabled
        } : body, body.op === "__service" ? "/api/service" : "/api/patchbay");
    }

    function request(method, body, path) {
        var current = ++generation;
        var xhr = new XMLHttpRequest();
        var done = false;
        busy = true;
        inFlight = [xhr];
        deadline.restart();
        function finish() {
            if (done)
                return;
            done = true;
            deadline.stop();
            inFlight = [];
            if (current >= appliedGeneration) {
                appliedGeneration = current;
                try {
                    var response = JSON.parse(xhr.responseText || "{}");
                    if (xhr.status >= 200 && xhr.status < 300) {
                        if (response.nodes)
                            snapshot = response;
                        error = response.error || "";
                    } else {
                        error = response.error || "Start on-air-core to connect.";
                    }
                } catch (_) {
                    error = "Start on-air-core to connect.";
                }
            }
            busy = false;
            updated();
            if (queue.length > 0)
                Qt.callLater(drain);
        }
        xhr.onreadystatechange = function () {
            if (xhr.readyState === XMLHttpRequest.DONE)
                finish();
        };
        xhr.open(method, "http://127.0.0.1:" + port + (path || "/api/patchbay"));
        xhr.setRequestHeader("Content-Type", "application/json");
        xhr.send(body ? JSON.stringify(body) : "");
    }

    Timer {
        id: deadline
        interval: 15000
        onTriggered: {
            var pending = root.inFlight.slice();
            for (var i = 0; i < pending.length; ++i)
                pending[i].abort();
            root.error = "The core is taking too long to respond. Try again.";
            root.busy = false;
            root.updated();
        }
    }
    Timer {
        interval: root.panelOpen ? 2000 : 10000
        running: true
        repeat: true
        onTriggered: root.refresh()
    }
    onPanelOpenChanged: if (panelOpen)
        refresh()
    Component.onCompleted: refresh()
    Component.onDestruction: {
        var pending = inFlight.slice();
        for (var i = 0; i < pending.length; ++i)
            pending[i].abort();
    }
}
