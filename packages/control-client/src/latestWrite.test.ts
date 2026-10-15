import assert from "node:assert/strict";
import { test } from "node:test";
import { createLatestWriteQueue } from "./latestWrite.ts";
import { deferred, tick } from "./testing.ts";

test("sends at once when idle and keeps only the newest value while busy", async () => {
  const first = deferred();
  const sent: number[] = [];
  const queue = createLatestWriteQueue<number>(async (v) => {
    sent.push(v);
    if (v === 1) await first.promise;
  });
  queue.push(1);
  queue.push(2);
  queue.push(3);
  assert.deepEqual(sent, [1]);
  assert.equal(queue.sending(), true);
  first.resolve();
  await tick();
  assert.deepEqual(sent, [1, 3]);
  assert.equal(queue.sending(), false);
});

test("a failed write gives way to the newer queued value; only the last failure rolls back", async () => {
  const first = deferred();
  const errors: Array<[number, number | undefined]> = [];
  let calls = 0;
  const queue = createLatestWriteQueue<number>(
    async () => {
      calls += 1;
      if (calls === 1) await first.promise;
      if (calls === 2) throw new Error("again");
    },
    (_err, value, committed) => errors.push([value, committed]),
  );
  queue.push(60, 50);
  queue.push(70, 60);
  first.reject(new Error("lost"));
  await tick();
  assert.equal(calls, 2);
  assert.deepEqual(errors, [[70, 50]]);
});

test("the rollback target follows the last accepted write; reset silences in-flight failures", async () => {
  const errors: Array<[number, number | undefined]> = [];
  let fail = false;
  const pending = deferred();
  const queue = createLatestWriteQueue<number>(
    async (v) => {
      if (v === 80) await pending.promise;
      if (fail) throw new Error("nope");
    },
    (_err, value, committed) => errors.push([value, committed]),
  );
  queue.push(60, 50);
  await tick();
  fail = true;
  queue.push(70, 60);
  await tick();
  assert.deepEqual(errors, [[70, 60]]);
  queue.push(80);
  queue.reset();
  pending.reject(new Error("stale"));
  await tick();
  assert.deepEqual(errors, [[70, 60]]);
});
