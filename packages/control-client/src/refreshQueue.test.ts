import assert from "node:assert/strict";
import { test } from "node:test";
import { createRefreshQueue } from "./refreshQueue.ts";
import { deferred, tick } from "./testing.ts";

test("a burst inside the window becomes one run of the union", async () => {
  const runs: string[][] = [];
  const queue = createRefreshQueue<string>(async (slices) => {
    runs.push(slices);
  }, 10);
  queue.schedule(["a"]);
  queue.schedule(["a", "b"]);
  await tick(2);
  assert.deepEqual(runs, []);
  await tick(20);
  assert.deepEqual(runs, [["a", "b"]]);
});

test("requests during a run become exactly one more run; refreshNow waits for its run", async () => {
  const runs: string[][] = [];
  const first = deferred();
  let gate: Promise<void> | null = first.promise;
  const queue = createRefreshQueue<string>(async (slices) => {
    runs.push(slices);
    if (gate) {
      const g = gate;
      gate = null;
      await g;
    }
  }, 10);
  const one = queue.refreshNow(["a"]);
  assert.equal(queue.inFlight(), true);
  let twoDone = false;
  const two = queue.refreshNow(["b"]).then(() => {
    twoDone = true;
  });
  queue.schedule(["c"], true);
  assert.deepEqual(runs, [["a"]]);
  await tick(0);
  assert.equal(twoDone, false);
  first.resolve();
  await one;
  await two;
  assert.deepEqual(runs, [["a"], ["b", "c"]]);
  assert.equal(queue.inFlight(), false);
});

test("clear drops pending work, the timer and releases waiters", async () => {
  const runs: string[][] = [];
  const queue = createRefreshQueue<string>(async (slices) => {
    runs.push(slices);
  }, 10);
  queue.schedule(["a"]);
  const gate = deferred();
  queue.schedule([], true);
  const waiting = queue.refreshNow(["b"]);
  runs.length = 0;
  queue.clear();
  await waiting;
  await tick(20);
  assert.deepEqual(runs, []);
  gate.resolve();
});
