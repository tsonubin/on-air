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
    // Media surface state: the CD deck and the active output, best-effort.
    property var cd: null
    property var activeOutput: null
    property string pairingPin: ""
    property int idleInterval: 10000
    // Toasts diff snapshots; the first observation of each value is a
    // baseline and stays quiet.
    property bool observed: false
    property bool pinObserved: false
    property var toastBurst: ({})
    property var mediaRequests: []
    signal updated
    signal toast(string title, string body, string urgency)

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

    function linkKey(link) {
        return link.source + "\n" + link.target;
    }
    function labelFor(snap, link) {
        var nodes = snap.nodes || [];
        for (var i = 0; i < nodes.length; ++i)
            if (nodes[i].id === link.target)
                return nodes[i].name;
        return link.target;
    }
    function acceptSnapshot(next) {
        if (observed)
            diffToasts(snapshot, next);
        observed = true;
        snapshot = next;
    }
    function diffToasts(previous, next) {
        var prevLinks = previous.links || [], nextLinks = next.links || [];
        var before = {}, seen = {}, i, key;
        for (i = 0; i < prevLinks.length; ++i)
            before[linkKey(prevLinks[i])] = prevLinks[i];
        for (i = 0; i < nextLinks.length; ++i) {
            var link = nextLinks[i];
            key = linkKey(link);
            seen[key] = true;
            var was = before[key];
            if (!was) {
                if (link.state === "connected")
                    noteToast("connected", labelFor(next, link));
            } else if (was.state !== link.state) {
                if (link.state === "connected" && was.state !== "paused")
                    noteToast("connected", labelFor(next, link));
                else if (link.state === "failed" || link.state === "unavailable")
                    noteToast("failed", labelFor(next, link));
            }
        }
        for (i = 0; i < prevLinks.length; ++i) {
            key = linkKey(prevLinks[i]);
            if (!seen[key])
                noteToast("disconnected", labelFor(previous, prevLinks[i]));
        }
        if (previous.enabled !== next.enabled)
            noteToast(next.enabled ? "resumed" : "paused", "Audio");
    }
    function noteToast(category, label) {
        var burst = Object.assign({}, toastBurst);
        var labels = burst[category] ? burst[category].slice() : [];
        if (labels.indexOf(label) < 0)
            labels.push(label);
        burst[category] = labels;
        toastBurst = burst;
        toastFlush.restart();
    }
    function flushToasts() {
        var burst = toastBurst;
        toastBurst = ({});
        var verbs = {
            connected: "connected",
            disconnected: "disconnected",
            failed: "failed to connect",
            paused: "paused",
            resumed: "resumed"
        };
        Object.keys(burst).forEach(function (category) {
            var labels = burst[category];
            var body = category === "pin" ? "Pairing PIN · " + labels[labels.length - 1] : labels.join(", ") + " " + verbs[category];
            toast("on-air", body, category === "failed" ? "normal" : "low");
        });
    }
    function acceptPin(pin) {
        if (!pin)
            return;
        if (!pinObserved) {
            pinObserved = true;
            pairingPin = pin;
        } else if (pin !== pairingPin) {
            pairingPin = pin;
            noteToast("pin", pin);
        }
    }
    function fetchJson(path, apply) {
        var xhr = new XMLHttpRequest();
        mediaRequests = mediaRequests.concat([xhr]);
        xhr.onreadystatechange = function () {
            if (xhr.readyState !== XMLHttpRequest.DONE)
                return;
            mediaRequests = mediaRequests.filter(function (item) {
                return item !== xhr;
            });
            if (xhr.status >= 200 && xhr.status < 300) {
                try {
                    apply(JSON.parse(xhr.responseText || "{}"));
                } catch (_) {}
            }
        };
        xhr.open("GET", "http://127.0.0.1:" + port + path);
        xhr.send("");
    }
    function postJson(path, body, done) {
        var xhr = new XMLHttpRequest();
        mediaRequests = mediaRequests.concat([xhr]);
        xhr.onreadystatechange = function () {
            if (xhr.readyState !== XMLHttpRequest.DONE)
                return;
            mediaRequests = mediaRequests.filter(function (item) {
                return item !== xhr;
            });
            if (done)
                done();
        };
        xhr.open("POST", "http://127.0.0.1:" + port + path);
        xhr.setRequestHeader("Content-Type", "application/json");
        xhr.send(JSON.stringify(body));
    }
    function refreshMedia() {
        fetchJson("/api/cd", function (data) {
            cd = data;
        });
        fetchJson("/api/outputs/active", function (data) {
            activeOutput = data && data.active ? data.active : null;
        });
        fetchJson("/api/pairing/pin", function (data) {
            acceptPin(data.pin);
        });
    }
    function cdControl(action) {
        postJson("/api/cd/control", {
            action: action
        }, refreshMedia);
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
                            acceptSnapshot(response);
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
        interval: root.panelOpen ? 2000 : root.idleInterval
        running: true
        repeat: true
        onTriggered: {
            root.refresh();
            root.refreshMedia();
        }
    }
    Timer {
        id: toastFlush
        interval: 500
        onTriggered: root.flushToasts()
    }
    onPanelOpenChanged: if (panelOpen) {
        refresh();
        refreshMedia();
    }
    Component.onCompleted: {
        refresh();
        refreshMedia();
    }
    Component.onDestruction: {
        var pending = inFlight.concat(mediaRequests);
        for (var i = 0; i < pending.length; ++i)
            pending[i].abort();
    }
}
