import assert from "node:assert/strict";
import { test } from "node:test";
import {
  BACKOFF_JITTER,
  backoffDelay,
  type ConnectionState,
  DEFAULT_MAX_BACKOFF_MS,
  DEFAULT_MIN_BACKOFF_MS,
  subscribeEvents,
  type WebSocketLike,
} from "./events.ts";

/** Scriptable stand-in for a WHATWG WebSocket. */
class FakeSocket implements WebSocketLike {
  static instances: FakeSocket[] = [];
  static failConstructor = false;
  onopen: WebSocketLike["onopen"] = null;
  onmessage: WebSocketLike["onmessage"] = null;
  onclose: WebSocketLike["onclose"] = null;
  onerror: WebSocketLike["onerror"] = null;
  closed = false;
  pingListeners: Array<() => void> = [];
  readonly url: string;

  constructor(url: string) {
    if (FakeSocket.failConstructor) throw new Error("boom");
    this.url = url;
    FakeSocket.instances.push(this);
  }

  close(): void {
    this.closed = true;
  }

  on(_event: "ping", listener: () => void): void {
    this.pingListeners.push(listener);
  }

  // Server-side script.
  open(): void {
    this.onopen?.({});
  }
  send(data: unknown): void {
    this.onmessage?.({ data });
  }
  ping(): void {
    for (const listener of this.pingListeners) listener();
  }
  dropFromServer(): void {
    this.onclose?.({ code: 1006 });
  }
  fail(): void {
    this.onerror?.({});
  }
}

function reset() {
  FakeSocket.instances = [];
  FakeSocket.failConstructor = false;
}

const sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

async function waitFor(check: () => boolean, label: string, timeoutMs = 2_000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (!check()) {
    if (Date.now() > deadline) throw new Error(`timed out waiting for ${label}`);
    await sleep(2);
  }
}

function harness(extra: Parameters<typeof subscribeEvents>[3] = {}) {
  reset();
  const events: unknown[] = [];
  const states: ConnectionState[] = [];
  const closes: string[] = [];
  let opens = 0;
  const sub = subscribeEvents(
    "http://10.0.0.2:47990",
    "tok",
    {
      onEvent: (ev) => events.push(ev),
      onOpen: () => {
        opens += 1;
      },
      onClose: (reason) => closes.push(reason),
      onStateChange: (state) => states.push(state),
    },
    {
      WebSocket: FakeSocket,
      stallMs: 0,
      minBackoffMs: 5,
      maxBackoffMs: 40,
      random: () => 0.5, // jitter factor of exactly 1
      ...extra,
    },
  );
  return {
    sub,
    events,
    states,
    closes,
    opens: () => opens,
    socket: (index: number) => FakeSocket.instances[index],
  };
}

test("backoffDelay doubles from 500 ms, caps at 10 s and jitters by +/-20 %", () => {
  const exact = () => 0.5;
  assert.equal(backoffDelay(0, DEFAULT_MIN_BACKOFF_MS, DEFAULT_MAX_BACKOFF_MS, exact), 500);
  assert.equal(backoffDelay(1, DEFAULT_MIN_BACKOFF_MS, DEFAULT_MAX_BACKOFF_MS, exact), 1000);
  assert.equal(backoffDelay(4, DEFAULT_MIN_BACKOFF_MS, DEFAULT_MAX_BACKOFF_MS, exact), 8000);
  assert.equal(backoffDelay(5, DEFAULT_MIN_BACKOFF_MS, DEFAULT_MAX_BACKOFF_MS, exact), 10_000);
  assert.equal(backoffDelay(20, DEFAULT_MIN_BACKOFF_MS, DEFAULT_MAX_BACKOFF_MS, exact), 10_000);
  const low = backoffDelay(0, 500, 10_000, () => 0);
  const high = backoffDelay(0, 500, 10_000, () => 0.999_999);
  assert.equal(low, Math.round(500 * (1 - BACKOFF_JITTER)));
  assert.equal(high, Math.round(500 * (1 + BACKOFF_JITTER)));
  assert.ok(backoffDelay(5, 500, 10_000, () => 0.999_999) <= 12_000);
});

test("open delivers parsed events and reports connecting then open", async () => {
  const h = harness();
  assert.equal(FakeSocket.instances.length, 1);
  assert.equal(h.socket(0).url, "ws://10.0.0.2:47990/api/ws?token=tok");
  assert.deepEqual(h.states, ["connecting"]);
  h.socket(0).open();
  assert.deepEqual(h.states, ["connecting", "open"]);
  assert.equal(h.opens(), 1);
  h.socket(0).send(JSON.stringify({ type: "LevelMeter", rms: 0.1, peak: 0.5 }));
  h.socket(0).send("not json");
  h.socket(0).send(new ArrayBuffer(2));
  assert.deepEqual(h.events, [{ type: "LevelMeter", rms: 0.1, peak: 0.5 }]);
  h.sub.close();
});

test("a server close reconnects with growing backoff and resets after open", async () => {
  const h = harness();
  h.socket(0).open();
  h.socket(0).dropFromServer();
  assert.deepEqual(h.states, ["connecting", "open", "reconnecting"]);
  assert.deepEqual(h.closes, ["remote"]);
  assert.equal(h.socket(0).closed, true);

  const t0 = Date.now();
  await waitFor(() => FakeSocket.instances.length === 2, "first retry");
  const firstDelay = Date.now() - t0;
  assert.ok(firstDelay < 40, `first retry should be ~5 ms, was ${firstDelay}`);

  // Fail again before opening: no second "reconnecting", longer wait.
  h.socket(1).fail();
  assert.deepEqual(h.states, ["connecting", "open", "reconnecting"]);
  assert.deepEqual(h.closes, ["remote", "error"]);
  const t1 = Date.now();
  await waitFor(() => FakeSocket.instances.length === 3, "second retry");
  const secondDelay = Date.now() - t1;
  assert.ok(secondDelay >= 8, `second retry should be ~10 ms, was ${secondDelay}`);

  // Opening resets the backoff: the next failure retries at the minimum again.
  h.socket(2).open();
  assert.equal(h.opens(), 2);
  assert.deepEqual(h.states, ["connecting", "open", "reconnecting", "open"]);
  h.socket(2).dropFromServer();
  const t2 = Date.now();
  await waitFor(() => FakeSocket.instances.length === 4, "third retry");
  assert.ok(Date.now() - t2 < 40);
  h.sub.close();
});

test("backoff never exceeds maxBackoffMs", async () => {
  const h = harness({ minBackoffMs: 2, maxBackoffMs: 10 });
  for (let i = 0; i < 6; i += 1) {
    const before = FakeSocket.instances.length;
    FakeSocket.instances[before - 1].fail();
    const t = Date.now();
    await waitFor(() => FakeSocket.instances.length === before + 1, `retry ${i}`);
    assert.ok(Date.now() - t < 60, `retry ${i} waited ${Date.now() - t} ms`);
  }
  h.sub.close();
});

test("a stalled socket is closed and replaced; frames and pings keep it alive", async () => {
  const h = harness({ stallMs: 30 });
  h.socket(0).open();
  // Keep it alive past the stall window with messages, then with pings.
  for (let i = 0; i < 4; i += 1) {
    await sleep(10);
    h.socket(0).send(JSON.stringify({ type: "LevelMeter", rms: 0, peak: 0 }));
  }
  for (let i = 0; i < 4; i += 1) {
    await sleep(10);
    h.socket(0).ping();
  }
  assert.equal(FakeSocket.instances.length, 1, "liveness frames must not trigger a reconnect");

  // Now go quiet.
  await waitFor(() => FakeSocket.instances.length === 2, "stall reconnect");
  assert.equal(h.socket(0).closed, true);
  assert.deepEqual(h.closes, ["stall"]);
  assert.deepEqual(h.states, ["connecting", "open", "reconnecting"]);
  h.sub.close();
});

test("the stall timer does not run before the socket opens", async () => {
  const h = harness({ stallMs: 10 });
  await sleep(40);
  assert.equal(FakeSocket.instances.length, 1);
  assert.deepEqual(h.closes, []);
  h.sub.close();
});

test("close() stops everything and never reconnects", async () => {
  const h = harness({ stallMs: 10 });
  h.socket(0).open();
  h.sub.close();
  assert.equal(h.socket(0).closed, true);
  assert.deepEqual(h.states, ["connecting", "open", "closed"]);
  // Late signals from the old socket and timers are ignored.
  h.socket(0).dropFromServer();
  h.socket(0).send(JSON.stringify({ type: "ServiceStateChanged", enabled: false }));
  await sleep(40);
  assert.equal(FakeSocket.instances.length, 1);
  assert.deepEqual(h.closes, []);
  assert.deepEqual(h.events, []);
  h.sub.close(); // idempotent
  assert.deepEqual(h.states, ["connecting", "open", "closed"]);
});

test("close() during a retry wait cancels the pending reconnect", async () => {
  const h = harness({ minBackoffMs: 20, maxBackoffMs: 20 });
  h.socket(0).open();
  h.socket(0).dropFromServer();
  h.sub.close();
  await sleep(50);
  assert.equal(FakeSocket.instances.length, 1);
});

test("a throwing WebSocket constructor is retried", async () => {
  reset();
  FakeSocket.failConstructor = true;
  const sub = subscribeEvents(
    "http://10.0.0.2:47990",
    undefined,
    { onEvent: () => {} },
    { WebSocket: FakeSocket, minBackoffMs: 5, maxBackoffMs: 5, random: () => 0.5 },
  );
  assert.equal(FakeSocket.instances.length, 0);
  FakeSocket.failConstructor = false;
  await waitFor(() => FakeSocket.instances.length === 1, "constructor retry");
  assert.equal(FakeSocket.instances[0].url, "ws://10.0.0.2:47990/api/ws");
  sub.close();
});

test("a Heartbeat text frame keeps the stall timer alive and reaches onEvent", async () => {
  const h = harness({ stallMs: 30 });
  h.socket(0).open();
  for (let i = 0; i < 6; i += 1) {
    await sleep(10);
    h.socket(0).send(JSON.stringify({ type: "Heartbeat" }));
  }
  assert.equal(FakeSocket.instances.length, 1, "heartbeats must not trigger a reconnect");
  assert.deepEqual(h.closes, []);
  assert.equal(h.events.length, 6);
  assert.deepEqual(h.events[0], { type: "Heartbeat" });
  h.sub.close();
});
