import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const physics = vm.createContext({});
vm.runInContext(
  readFileSync(new URL("../Rope.js", import.meta.url), "utf8").replace(/^\.pragma library\s*/, ""),
  physics,
);
const bounds = { left: 0, right: 100, top: 0, bottom: 120 };

test("an anchored wire hangs below both ports and eventually sleeps", () => {
  const rope = physics.create({ x: 10, y: 20 }, { x: 90, y: 20 });
  for (let i = 0; i < 700; i++) physics.step(rope, 1 / 60, bounds);
  assert.equal(rope.points[0].x, 10);
  assert.equal(rope.points[0].y, 20);
  assert.equal(rope.points.at(-1).x, 90);
  assert.equal(rope.points.at(-1).y, 20);
  assert.ok(rope.points[8].y > 35, "gravity must produce visible sag");
  assert.equal(rope.awake, false, "idle cables must stop consuming frames");
});

test("a moved endpoint wakes the rope and frame stalls cannot explode it", () => {
  const rope = physics.create({ x: 10, y: 20 }, { x: 90, y: 20 });
  physics.settle(rope, bounds);
  physics.anchor(rope, { x: 10, y: 20 }, { x: 80, y: 65 });
  assert.equal(rope.awake, true);
  for (let i = 0; i < 700; i++) physics.step(rope, i % 3 ? 1 / 144 : 4, bounds);
  for (const point of rope.points) {
    assert.ok(Number.isFinite(point.x) && Number.isFinite(point.y));
    assert.ok(point.x >= bounds.left && point.x <= bounds.right);
    assert.ok(point.y >= bounds.top && point.y <= bounds.bottom);
  }
  assert.equal(rope.points.at(-1).y, 65);
  assert.equal(rope.awake, false);
});

test("a click near a curved segment finds its route without hitting distant points", () => {
  const rope = physics.create({ x: 10, y: 20 }, { x: 90, y: 20 });
  physics.settle(rope, bounds);
  assert.equal(physics.hit(rope, rope.points[8], 7), true);
  assert.equal(physics.hit(rope, { x: 50, y: 0 }, 7), false);
});

test("reduced-motion settling retains gravity with no running animation", () => {
  const rope = physics.create({ x: 10, y: 20 }, { x: 90, y: 20 });
  physics.settle(rope, bounds);
  assert.equal(rope.awake, false);
  assert.ok(rope.points[8].y > 35);
  const before = JSON.stringify(rope.points);
  physics.step(rope, 1, bounds);
  assert.equal(JSON.stringify(rope.points), before);
});
