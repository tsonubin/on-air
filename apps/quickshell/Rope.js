.pragma library

// Verlet integration with positional distance constraints. Pixel units;
// fixed 120 Hz steps keep the wire stable across rendering frame rates.
function create(start, end) {
    var dx = end.x - start.x, dy = end.y - start.y;
    var span = Math.sqrt(dx * dx + dy * dy);
    var sag = Math.max(14, Math.min(32, span * 0.32));
    var points = [];
    var length = 0;
    for (var i = 0; i <= 16; ++i) {
        var t = i / 16;
        var p = {x: start.x + dx * t, y: start.y + dy * t + 4 * sag * t * (1 - t)};
        p.px = p.x; p.py = p.y;
        if (i > 0) length += distance(points[i - 1], p);
        points.push(p);
    }
    return {points: points, length: length, start: start, end: end, accumulator: 0, awake: true, quiet: 0, age: 0};
}

function distance(a, b) { var x = b.x - a.x, y = b.y - a.y; return Math.sqrt(x * x + y * y); }

function anchor(rope, start, end) {
    if (distance(rope.start, start) + distance(rope.end, end) > 0.1) {
        rope.start = start; rope.end = end; rope.awake = true; rope.quiet = 0; rope.age = 0;
        // Slack adapts when the pointer stretches the cable, then remains
        // fixed while gravity finds its new hanging shape.
        rope.length = Math.max(distance(start, end) * 1.12, Math.min(rope.length, distance(start, end) * 1.5 + 16));
    }
}

function step(rope, elapsed, bounds) {
    if (!rope.awake) return false;
    rope.accumulator += Math.min(elapsed, 0.05);
    while (rope.accumulator >= 1 / 120) {
        integrate(rope, bounds);
        rope.accumulator -= 1 / 120;
    }
    return rope.awake;
}

function integrate(rope, bounds) {
    var ps = rope.points, count = ps.length - 1, rest = rope.length / count;
    var before = ps.map(function(p) { return {x: p.x, y: p.y}; });
    for (var i = 1; i < count; ++i) {
        var p = ps[i], vx = (p.x - p.px) * 0.975, vy = (p.y - p.py) * 0.975;
        p.px = p.x; p.py = p.y;
        p.x += vx; p.y += vy + 1150 / (120 * 120);
    }
    for (var pass = 0; pass < 12; ++pass) {
        ps[0].x = rope.start.x; ps[0].y = rope.start.y;
        ps[count].x = rope.end.x; ps[count].y = rope.end.y;
        for (var j = 0; j < count; ++j) {
            var a = ps[j], b = ps[j + 1], dx = b.x - a.x, dy = b.y - a.y;
            var len = Math.sqrt(dx * dx + dy * dy);
            if (len < 0.00001) continue;
            var correction = (len - rest) / len;
            var wa = j === 0 ? 0 : j + 1 === count ? 1 : 0.5;
            var wb = j + 1 === count ? 0 : j === 0 ? 1 : 0.5;
            a.x += dx * correction * wa; a.y += dy * correction * wa;
            b.x -= dx * correction * wb; b.y -= dy * correction * wb;
        }
        if (bounds) for (var k = 1; k < count; ++k) {
            ps[k].x = Math.max(bounds.left, Math.min(bounds.right, ps[k].x));
            ps[k].y = Math.max(bounds.top, Math.min(bounds.bottom, ps[k].y));
        }
    }
    ps[0].x = rope.start.x; ps[0].y = rope.start.y;
    ps[count].x = rope.end.x; ps[count].y = rope.end.y;
    var motion = 0;
    for (var m = 1; m < count; ++m) motion = Math.max(motion, distance(before[m], ps[m]));
    rope.quiet = motion < 0.025 ? rope.quiet + 1 : 0;
    rope.age++;
    if (rope.quiet > 24 || rope.age > 600) rope.awake = false;
}

function settle(rope, bounds) {
    for (var i = 0; i < 601 && rope.awake; ++i) integrate(rope, bounds);
    rope.awake = false;
}

// Flattened polyline of the drawn path, sampled from the same quadratics
// CableLayer paints: curves through the midpoints between control points.
function curve(rope) {
    var ps = rope.points, out = [{x: ps[0].x, y: ps[0].y}];
    for (var i = 1; i < ps.length - 1; ++i) {
        var c = ps[i], start = out[out.length - 1];
        var end = {x: (c.x + ps[i + 1].x) / 2, y: (c.y + ps[i + 1].y) / 2};
        for (var s = 1; s <= 6; ++s) {
            var t = s / 6, u = 1 - t;
            out.push({
                x: u * u * start.x + 2 * u * t * c.x + t * t * end.x,
                y: u * u * start.y + 2 * u * t * c.y + t * t * end.y
            });
        }
    }
    out.push({x: ps[ps.length - 1].x, y: ps[ps.length - 1].y});
    return out;
}

function hit(rope, point, radius) {
    var ps = curve(rope);
    for (var i = 0; i < ps.length - 1; ++i) {
        var a = ps[i], b = ps[i + 1], dx = b.x - a.x, dy = b.y - a.y;
        var t = Math.max(0, Math.min(1, ((point.x - a.x) * dx + (point.y - a.y) * dy) / Math.max(0.001, dx * dx + dy * dy)));
        if (distance(point, {x: a.x + dx * t, y: a.y + dy * t}) <= radius) return true;
    }
    return false;
}
