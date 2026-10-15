import assert from "node:assert/strict";
import { test } from "node:test";
import type { OutputInfo, StatusResponse } from "@on-air/api-types";
import { HttpError } from "./http.ts";
import {
  ALL_SLICES,
  type CoreSnapshot,
  createStaleGuard,
  EMPTY_SNAPSHOT,
  judgeResults,
  mergeResults,
  readSlices,
  reduceEvent,
  type SliceReaders,
  type SliceResults,
} from "./snapshot.ts";
import { idleCd, sonos } from "./testing.ts";

const status: StatusResponse = { status: "ok", version: "0.2.0", service_enabled: true };

function readers(overrides: Partial<SliceReaders> = {}): SliceReaders & { calls: string[] } {
  const calls: string[] = [];
  const track =
    <T>(name: string, value: T) =>
    async () => {
      calls.push(name);
      return value;
    };
  return {
    calls,
    status: track("status", status),
    inputs: track("inputs", ["Mock Monitor"]),
    activeInput: track("activeInput", { name: "Mock Monitor", backend: "cpal-default" }),
    outputs: track("outputs", [sonos]),
    activeOutput: track("activeOutput", null),
    volume: track("volume", 42),
    gains: track("gains", [0, 0, 0, 0, 0]),
    sampleRate: track("sampleRate", {
      sample_rate_hz: 44100,
      input: { sample_rate_hz: 44100, supported_hz: [44100] },
      output: { sample_rate_hz: 48000, supported_hz: [48000] },
    }),
    pin: track("pin", "123456"),
    airplayMode: track("airplayMode", "owntone"),
    cd: track("cd", idleCd),
    ...overrides,
  };
}

const envelope = (code: string, httpStatus: number) =>
  new HttpError("/api/x", httpStatus, JSON.stringify({ error: "", code }));

test("readSlices reads status first and skips the rest of a paused core", async () => {
  const r = readers({
    status: async () => ({ ...status, service_enabled: false }),
  });
  const results = await readSlices(r, ALL_SLICES);
  assert.equal(results.status?.status, "fulfilled");
  assert.equal(results.volume, undefined);
  assert.deepEqual(r.calls, []);
  assert.equal(judgeResults(results, ALL_SLICES).kind, "settled");
});

test("readSlices still reads the other slices when status fails", async () => {
  const r = readers({
    status: async () => {
      throw new TypeError("Failed to fetch");
    },
  });
  const results = await readSlices(r, ["status", "volume"]);
  assert.equal(results.volume?.status, "fulfilled");
  assert.deepEqual(judgeResults(results, ["status", "volume"]), {
    kind: "settled",
    phase: "reconnecting",
    partial: false,
  });
});

test("mergeResults keeps rejected and stale slices and unwraps the active input", async () => {
  const results = await readSlices(
    readers({
      outputs: async () => {
        throw new Error("slow");
      },
    }),
    ["inputs", "activeInput", "outputs", "volume"],
  );
  const prev = { ...EMPTY_SNAPSHOT, outputs: [sonos], volume: 10 };
  const next = mergeResults(prev, results, (slice) => slice === "volume");
  assert.deepEqual(next.inputs, ["Mock Monitor"]);
  assert.equal(next.activeInput, "Mock Monitor");
  assert.equal(next.outputs, prev.outputs);
  assert.equal(next.volume, 10);
});

test("judgeResults: a 401 anywhere wins, a 503 is paused, optional-only outages are inconclusive", () => {
  const unauthorized: SliceResults = {
    status: { status: "rejected", reason: new Error("timeout") },
    inputs: { status: "rejected", reason: envelope("not_paired", 401) },
  };
  assert.equal(judgeResults(unauthorized, ["status", "inputs"]).kind, "unauthorized");
  const paused: SliceResults = {
    status: { status: "fulfilled", value: status },
    volume: { status: "rejected", reason: envelope("service_paused", 503) },
  };
  assert.deepEqual(judgeResults(paused, ["status", "volume"]), {
    kind: "settled",
    phase: "paused",
    partial: false,
  });
  const optional: SliceResults = { cd: { status: "rejected", reason: new Error("offline") } };
  assert.deepEqual(judgeResults(optional, ["cd"]), { kind: "inconclusive" });
  const partial: SliceResults = {
    status: { status: "fulfilled", value: status },
    gains: { status: "rejected", reason: new Error("slow") },
    cd: { status: "rejected", reason: new HttpError("/api/cd", 404) },
  };
  assert.deepEqual(judgeResults(partial, ["status", "gains", "cd"]), {
    kind: "settled",
    phase: "connected",
    partial: true,
  });
});

test("the stale guard stamps slices per touch and forgets on reset", () => {
  const guard = createStaleGuard<"a" | "b">();
  const since = guard.now();
  guard.touch("a");
  assert.equal(guard.isStale("a", since), true);
  assert.equal(guard.isStale("b", since), false);
  assert.equal(guard.isStale("a", guard.now()), false);
  guard.reset();
  assert.equal(guard.isStale("a", since), false);
});

test("reduceEvent applies output, input, disc and catalog events", () => {
  const homepod: OutputInfo = { ...sonos, id: "hp-1", name: "Bedroom", transport: "airplay" };
  let snapshot: CoreSnapshot = { ...EMPTY_SNAPSHOT, outputs: [sonos, homepod], cd: { ...idleCd } };

  let r = reduceEvent(snapshot, {
    type: "OutputStateChanged",
    transport: "sonos",
    device_name: "Mock Sonos",
    active: true,
  });
  assert.deepEqual(r.next.activeOutput, {
    transport: "sonos",
    device_name: "Mock Sonos",
    device_id: "uuid:mock-sonos",
    state: "live",
  });
  assert.deepEqual(r.refresh, ["volume"]);
  snapshot = r.next;

  r = reduceEvent(snapshot, {
    type: "OutputStateChanged",
    transport: "airplay",
    device_name: "Bedroom",
    active: false,
  });
  assert.equal(
    r.next.activeOutput,
    snapshot.activeOutput,
    "a stop for another transport is ignored",
  );
  r = reduceEvent(snapshot, {
    type: "OutputStateChanged",
    transport: "sonos",
    device_name: "Mock Sonos",
    active: false,
  });
  assert.equal(r.next.activeOutput, null);

  r = reduceEvent(snapshot, { type: "InputStateChanged", name: "Line In", active: false });
  assert.equal(r.next.activeInput, snapshot.activeInput);
  r = reduceEvent(snapshot, {
    type: "InputStateChanged",
    name: "Line In",
    active: false,
    error: "died",
  });
  assert.equal(r.inputStopped, true);
  assert.deepEqual(r.refresh, ["inputs", "activeInput", "sampleRate"]);

  const disc = {
    type: "CdStateChanged" as const,
    present: true,
    playing: true,
    track: 1,
    track_count: 9,
    position_ms: 0,
    duration_ms: 1000,
  };
  r = reduceEvent(snapshot, disc);
  assert.equal(r.next.cd?.present, true);
  assert.deepEqual(r.refresh, ["cd", "inputs", "activeInput"]);
  r = reduceEvent(r.next, { ...disc, track: 2 });
  assert.deepEqual(r.refresh, [], "a track change needs no refresh");

  r = reduceEvent(snapshot, {
    type: "DeviceJoined",
    transport: "sonos",
    id: "uuid:new",
    name: "Patio",
  });
  assert.equal(r.next.outputs?.length, 3);
  assert.deepEqual(r.refresh, ["outputs"]);
  r = reduceEvent(r.next, { type: "DeviceLeft", transport: "airplay", id: "hp-1" });
  assert.deepEqual(
    r.next.outputs?.map((o) => o.id),
    ["uuid:mock-sonos", "uuid:new"],
  );
  assert.deepEqual(r.refresh, []);

  r = reduceEvent(snapshot, { type: "ServiceStateChanged", enabled: false });
  assert.equal(r.serviceEnabled, false);
  assert.deepEqual(r.refresh, []);
  r = reduceEvent(snapshot, { type: "Heartbeat" });
  assert.equal(r.next, snapshot);
});
