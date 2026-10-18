import type { ApiErrorBody, TuningStatus, Transport, WsEvent } from "@on-air/api-types";
import { expect, test } from "@playwright/test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import {
  activateOutput,
  addSyncGroupMember,
  createSyncGroup,
  deactivateOutput,
  dissolveSyncGroup,
  getAudioQuality,
  getSync,
  HttpError,
  leaveSyncGroupMember,
  listOutputs,
  runAudioBenchmark,
  setAudioQuality,
  setSyncTrim,
  setVolume,
  startSyncGroup,
  stopSyncGroup,
  subscribeEvents,
} from "../../packages/control-client/src/index.ts";
import { type MockCore, startMockCore } from "../shared/mockCore.ts";
import { playSyncGroup, trimFirstOutput } from "../shared/scenarios.ts";

// The sync, sync-group, DSP-quality and first-use-tuning surface driven
// through the shared control client — the client the desktop and phone
// apps use — against the mock core playwright.config.ts starts. Loopback
// callers need no pairing token.
const API = "http://127.0.0.1:47990";
// A scratch core for the tests that must see a fresh machine. Port 47990
// is the shared core, 47992/47993 the mobile suites', so 47994 is ours.
const ISOLATED_PORT = Number(process.env.E2E_ISOLATED_PORT ?? 47994);

async function firstOutputId(base: string, transport: Transport): Promise<string> {
  const outputs = await listOutputs(base);
  const device = outputs.find((o) => o.transport === transport);
  if (!device) throw new Error(`missing ${transport}`);
  return device.id;
}

/** The one error envelope, as a raw HTTP caller sees it. */
async function expectEnvelope(
  response: Response,
  status: number,
  code: string,
): Promise<ApiErrorBody> {
  expect(response.status).toBe(status);
  expect(response.headers.get("content-type")).toContain("application/json");
  const body = (await response.json()) as ApiErrorBody;
  expect(typeof body.error).toBe("string");
  expect(body.error.length).toBeGreaterThan(0);
  expect(body.code).toBe(code);
  return body;
}

/** A control-client call must refuse with `HttpError` carrying the envelope. */
async function expectRefusal(
  promise: Promise<unknown>,
  status: number,
  code: string,
): Promise<HttpError> {
  try {
    await promise;
  } catch (error) {
    expect(error, `expected HttpError, got ${String(error)}`).toBeInstanceOf(HttpError);
    const httpError = error as HttpError;
    expect(httpError.status).toBe(status);
    expect(httpError.code).toBe(code);
    expect(httpError.body.length).toBeGreaterThan(0);
    return httpError;
  }
  throw new Error(`expected a ${status} ${code} refusal, got success`);
}

async function getTuning(base: string): Promise<TuningStatus> {
  const response = await fetch(`${base}/api/audio/tuning`);
  expect(response.status).toBe(200);
  return (await response.json()) as TuningStatus;
}

test("sync: a trim is per output, manual, and clamped at zero latency", { tag: "@api" }, async () => {
  const { trimmed, row } = await trimFirstOutput(API, "sonos", 250);
  expect(trimmed).toEqual({ trim_ms: 250, effective_latency_ms: 1250 });
  expect(row.trim_ms).toBe(250);
  expect(row.default_latency_ms).toBe(1000);
  expect(row.effective_latency_ms).toBe(1250);
  // A trim is not a measurement: the model never claims otherwise.
  expect(row.calibrated).toBe(false);
  // Nothing to measure drift against in mock mode: the field stays absent.
  expect(row.drift_ppm).toBeUndefined();

  // Each output keeps its own trim, and a trim larger than the transport's
  // default latency clamps the effective latency at zero.
  const { row: airplayRow } = await trimFirstOutput(API, "airplay", -3000);
  expect(airplayRow.trim_ms).toBe(-3000);
  expect(airplayRow.effective_latency_ms).toBe(0);
  const sync = await getSync(API);
  const sonosRow = sync.outputs.find((row) => row.transport === "sonos");
  expect(sonosRow?.trim_ms).toBe(250);
});

test("sync: trim refuses out-of-range values and unknown transports", { tag: "@api" }, async () => {
  const sonos = await firstOutputId(API, "sonos");
  const tooFar = await expectRefusal(
    setSyncTrim(API, { transport: "sonos", device_id: sonos, trim_ms: 10_001 }),
    400,
    "invalid_request",
  );
  expect(tooFar.message).toContain("trim_ms must be between -10000 and 10000");

  // The client's types cannot spell this; a hand-rolled caller still gets
  // the one envelope.
  const response = await fetch(`${API}/api/sync/trim`, {
    method: "PUT",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ transport: "warp", device_id: "u", trim_ms: 1 }),
  });
  const body = await expectEnvelope(response, 400, "invalid_request");
  expect(body.error).toContain("unknown transport: warp");
});

test("sync groups: one start brings every member to one epoch", { tag: "@api" }, async () => {
  await trimFirstOutput(API, "sonos", 250);
  await trimFirstOutput(API, "airplay", 0);

  const events: WsEvent[] = [];
  let opened!: () => void;
  const open = new Promise<void>((resolve) => (opened = resolve));
  const subscription = subscribeEvents(API, undefined, {
    onEvent: (event) => events.push(event),
    onOpen: () => opened(),
  });
  let groupId: number | undefined;
  try {
    await open;
    ({ groupId } = await playSyncGroup(API, "Downstairs"));
    const group = (await getSync(API)).groups?.find((candidate) => candidate.id === groupId);
    expect(group?.name).toBe("Downstairs");
    expect(group?.playing).toBe(true);
    // A single start: every member plays together, each still its own
    // speaker with its own transport and trim.
    expect(group?.members.map((m) => [m.transport, m.playing])).toEqual([
      ["sonos", true],
      ["airplay", true],
    ]);
    expect(group?.members.map((m) => m.device_name)).toEqual([
      "Mock Sonos",
      "Living Room AirPlay",
    ]);

    const sync = await getSync(API);
    const rows = sync.outputs.filter((row) => row.group_id === groupId);
    expect(rows.map((row) => row.transport).sort()).toEqual(["airplay", "sonos"]);
    for (const row of rows) {
      expect(row.state).toBe("syncing");
      expect(row.calibrated).toBe(false);
    }
    const sonosRow = rows.find((row) => row.transport === "sonos");
    const airplayRow = rows.find((row) => row.transport === "airplay");
    // Each keeps its own trim inside the group.
    expect(sonosRow?.trim_ms).toBe(250);
    expect(airplayRow?.trim_ms).toBe(0);
    // The delay lines align the members on one epoch: the fast transport
    // waits for the slow one, never the other way round.
    expect(sonosRow?.delay_frames).toBeGreaterThan(0);
    expect(airplayRow?.delay_frames).toBe(0);

    // Every lifecycle change tells clients to re-read the one read model.
    await expect
      .poll(() => events.some((event) => event.type === "SyncGroupChanged" && event.group_id === groupId))
      .toBe(true);

    // Stopping keeps the group configured — it is simply no longer playing.
    await stopSyncGroup(API, groupId);
    const stopped = (await getSync(API)).groups?.find((candidate) => candidate.id === groupId);
    expect(stopped?.playing).toBe(false);
    expect(stopped?.members.map((m) => [m.transport, m.playing])).toEqual([
      ["sonos", false],
      ["airplay", false],
    ]);
  } finally {
    subscription.close();
    if (groupId !== undefined) {
      await stopSyncGroup(API, groupId).catch(() => {});
      await dissolveSyncGroup(API, groupId).catch(() => {});
    }
  }
});

test("sync groups: an output already in another group is refused", { tag: "@api" }, async () => {
  const sonos = await firstOutputId(API, "sonos");
  const downstairs = await createSyncGroup(API, "Downstairs");
  await addSyncGroupMember(API, downstairs.id, { transport: "sonos", device_id: sonos });
  // Re-adding a member of this same group is a no-op...
  await addSyncGroupMember(API, downstairs.id, { transport: "sonos", device_id: sonos });

  try {
    const kitchen = await createSyncGroup(API, "Kitchen");
    try {
      // ...but a second group may not claim an output the first one has.
      const conflict = await expectRefusal(
        addSyncGroupMember(API, kitchen.id, { transport: "sonos", device_id: sonos }),
        409,
        "conflict",
      );
      expect(conflict.message).toContain("this output already plays in group `Downstairs`");
      const sync = await getSync(API);
      const kitchenView = sync.groups?.find((group) => group.id === kitchen.id);
      expect(kitchenView?.members).toEqual([]);
    } finally {
      await dissolveSyncGroup(API, kitchen.id);
    }
  } finally {
    await dissolveSyncGroup(API, downstairs.id);
  }
});

test("sync groups: leaving stops just that member; the others keep playing", { tag: "@api" }, async () => {
  const sonos = await firstOutputId(API, "sonos");
  const { groupId } = await playSyncGroup(API, "Downstairs");
  try {
    await leaveSyncGroupMember(API, groupId, { transport: "sonos", device_id: sonos });
    const sync = await getSync(API);
    const group = sync.groups?.find((candidate) => candidate.id === groupId);
    expect(group?.members.map((m) => m.transport)).toEqual(["airplay"]);
    expect(group?.members[0]?.playing).toBe(true);
    const sonosRow = sync.outputs.find(
      (row) => row.transport === "sonos" && row.device_id === sonos,
    );
    expect(sonosRow?.group_id).toBeUndefined();
    expect(sonosRow?.state).toBe("free_running");
    expect(sonosRow?.delay_frames).toBe(0);
  } finally {
    await dissolveSyncGroup(API, groupId);
  }
});

test("sync groups: the lifecycle refuses wrong names, members and starts", { tag: "@api" }, async () => {
  const sonos = await firstOutputId(API, "sonos");
  const noName = await expectRefusal(createSyncGroup(API, "   "), 400, "invalid_request");
  expect(noName.message).toContain("a group name must be 1 to 64 characters");
  await expectRefusal(createSyncGroup(API, "x".repeat(65)), 400, "invalid_request");
  await expectRefusal(
    addSyncGroupMember(API, 999_999, { transport: "sonos", device_id: sonos }),
    404,
    "not_found",
  );

  const group = await createSyncGroup(API, "Test bench");
  try {
    const noDevice = await expectRefusal(
      addSyncGroupMember(API, group.id, { transport: "sonos", device_id: "uuid:nope" }),
      404,
      "not_found",
    );
    expect(noDevice.message).toContain("output device not found");
    const empty = await expectRefusal(startSyncGroup(API, group.id), 400, "invalid_request");
    expect(empty.message).toContain("a sync group needs at least one member to start");
    const notAMember = await expectRefusal(
      leaveSyncGroupMember(API, group.id, { transport: "sonos", device_id: sonos }),
      404,
      "not_found",
    );
    expect(notAMember.message).toContain("this output does not play in that sync group");
  } finally {
    await dissolveSyncGroup(API, group.id);
  }
  await expectRefusal(dissolveSyncGroup(API, group.id), 404, "not_found");
});

test("quality: a profile choice wins; an unknown profile changes nothing", { tag: "@api" }, async () => {
  const before = await getAudioQuality(API);
  expect(before.profiles).toEqual(["economy", "balanced", "high"]);

  await setAudioQuality(API, "economy");
  const economy = await getAudioQuality(API);
  expect(economy.quality).toBe("economy");
  // A choice does not rewrite the measured verdict.
  expect(economy.recommended).toBe(before.recommended);

  const response = await fetch(`${API}/api/audio/quality`, {
    method: "PUT",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ quality: "turbo" }),
  });
  await expectEnvelope(response, 400, "invalid_request");
  expect((await getAudioQuality(API)).quality).toBe("economy");

  await setAudioQuality(API, "high");
  expect((await getAudioQuality(API)).quality).toBe("high");
});

test("quality: the audio check's verdict becomes the recommendation", { tag: "@api" }, async () => {
  // A user's explicit choice survives the check: the verdict is advice,
  // applied only to settings nobody has touched.
  await setAudioQuality(API, "balanced");
  const report = await runAudioBenchmark(API);
  expect(report.recommended_quality).toBeTruthy();
  expect([0, 50, 100, 200, 400]).toContain(report.recommended_buffer_ms);
  expect(report.input_hz).toBeGreaterThan(0);
  expect(report.output_hz).toBeGreaterThan(0);
  expect(report.worst_case_ms).toBeGreaterThanOrEqual(0);

  const after = await getAudioQuality(API);
  expect(after.recommended).toBe(report.recommended_quality);
  expect(after.quality).toBe("balanced");
});

test("tuning: a check tunes the machine exactly once", { tag: "@api" }, async () => {
  const configDir = mkdtempSync(path.join(tmpdir(), "on-air-tuning-"));
  const core: MockCore = await startMockCore({
    port: ISOLATED_PORT,
    env: { ON_AIR_CONFIG_DIR: configDir },
  });
  try {
    // A fresh machine: no check has ever completed here.
    const before = await getTuning(core.base);
    expect(before.tuned).toBe(false);
    expect(before.automatic).toBeNull();
    expect(before.quality_override).toBeNull();
    expect(before.recommended_quality).toBeNull();
    expect(before.capture_buffer_ms).toBe(0);
    expect(before.profiles).toEqual(["economy", "balanced", "high"]);
    expect(before.quality).toBe((await getAudioQuality(core.base)).quality);

    // One completed check is all "tuned" needs — a user pressing the
    // button has seen the verdict, so first-use tuning must never run here.
    const report = await runAudioBenchmark(core.base);
    const after = await getTuning(core.base);
    expect(after.tuned).toBe(true);
    expect(after.recommended_quality).toBe(report.recommended_quality);
    expect(after.automatic).toBeNull();
  } finally {
    await core.stop();
  }
});

test("error envelope: unknown routes, wrong methods and missing outputs", { tag: "@api" }, async () => {
  const unknownRoute = await fetch(`${API}/api/nope`);
  const body = await expectEnvelope(unknownRoute, 404, "not_found");
  expect(body.error).toBe("no such route");

  const wrongMethod = await fetch(`${API}/api/sync`, { method: "DELETE" });
  const methodBody = await expectEnvelope(wrongMethod, 405, "method_not_allowed");
  expect(methodBody.error).toContain("does not accept that method");

  const malformed = await fetch(`${API}/api/sync/trim`, {
    method: "PUT",
    headers: { "content-type": "application/json" },
    body: "not json",
  });
  await expectEnvelope(malformed, 400, "invalid_request");

  // Volume needs a live output; without one the refusal names it.
  await deactivateOutput(API);
  await expectRefusal(setVolume(API, 20), 404, "no_active_output");

  const missing = await expectRefusal(activateOutput(API, "sonos", "uuid:nope"), 404, "not_found");
  expect(missing.message).toContain("output device not found");
});
