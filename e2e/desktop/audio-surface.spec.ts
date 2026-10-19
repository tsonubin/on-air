import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import type { ApiErrorBody, Transport, TuningStatus, WsEvent } from "@on-air/api-types";
import { expect, test } from "@playwright/test";
import {
  activateInput,
  activateOutput,
  addSyncGroupMember,
  createSyncGroup,
  deactivateOutput,
  dissolveSyncGroup,
  getActiveOutput,
  getAudioQuality,
  getSync,
  HttpError,
  leaveSyncGroupMember,
  listOutputs,
  runAudioBenchmark,
  setAudioQuality,
  setChannelMode,
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

/** The WAV header of the live radio stream, as the speaker would pull it. */
async function readWavHeader(base: string): Promise<{
  /** The 44 raw header bytes, for byte-identity comparisons. */
  bytes: Uint8Array;
  channels: number;
  sampleRate: number;
  byteRate: number;
  blockAlign: number;
}> {
  const radio = (await (await fetch(`${base}/api/mock/radio`)).json()) as { url: string };
  const response = await fetch(`${base}${radio.url}`);
  expect(response.status).toBe(200);
  const reader = response.body?.getReader();
  expect(reader).toBeTruthy();
  if (!reader) throw new Error("expected a readable body");
  const chunks: Uint8Array[] = [];
  let size = 0;
  while (size < 44) {
    const { value } = await reader.read();
    expect(value).toBeTruthy();
    if (!value) break;
    chunks.push(value);
    size += value.length;
  }
  await reader.cancel();
  const buffered = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) {
    buffered.set(chunk, offset);
    offset += chunk.length;
  }
  const bytes = buffered.slice(0, 44);
  const view = new DataView(bytes.buffer);
  return {
    bytes,
    channels: view.getUint16(22, true),
    sampleRate: view.getUint32(24, true),
    byteRate: view.getUint32(28, true),
    blockAlign: view.getUint16(32, true),
  };
}

test("sync: a trim is per output, manual, and clamped at zero latency", {
  tag: "@api",
}, async () => {
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
  const { promise: open, resolve: opened } = Promise.withResolvers<void>();
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
    expect(group?.members.map((m) => m.device_name)).toEqual(["Mock Sonos", "Living Room AirPlay"]);

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
      .poll(() =>
        events.some((event) => event.type === "SyncGroupChanged" && event.group_id === groupId),
      )
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

test("sync groups: leaving stops just that member; the others keep playing", {
  tag: "@api",
}, async () => {
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

test("sync groups: the lifecycle refuses wrong names, members and starts", {
  tag: "@api",
}, async () => {
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

test("quality: a profile choice wins; an unknown profile changes nothing", {
  tag: "@api",
}, async () => {
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

test("error envelope: unknown routes, wrong methods and missing outputs", {
  tag: "@api",
}, async () => {
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

test("channel mode: stereo on a known-mono device is refused with an explanation", {
  tag: "@api",
}, async () => {
  // "This computer" outputs report their channel count honestly, so the
  // core knows this one plays mono before anything is sent to it.
  const outputs = await listOutputs(API);
  const speaker = outputs.find((output) => output.id === "alsa#mono");
  expect(speaker?.transport).toBe("local");
  expect(speaker?.name).toBe("Mono Speaker");
  expect(speaker?.channels).toBe(1);

  // The refusal names what happened and what to do — never a silent
  // downmix of the user's choice.
  const refused = await expectRefusal(
    setChannelMode(API, { transport: "local", device_id: "alsa#mono", mode: "stereo" }),
    400,
    "invalid_request",
  );
  expect((JSON.parse(refused.body) as ApiErrorBody).error).toBe(
    "Mono Speaker plays mono only; stereo would drop the right channel. Choose Auto or Mono.",
  );
  // The refused selection was not saved.
  expect(
    (await listOutputs(API)).find((output) => output.id === "alsa#mono")?.channel_mode,
  ).toBeFalsy();

  // mono fits a mono device and is saved per output; other outputs are
  // untouched by one output's mode.
  await setChannelMode(API, { transport: "local", device_id: "alsa#mono", mode: "mono" });
  expect((await listOutputs(API)).find((output) => output.id === "alsa#mono")?.channel_mode).toBe(
    "mono",
  );
  expect(
    (await listOutputs(API)).find((output) => output.id === "uuid:mock-sonos")?.channel_mode,
  ).toBeFalsy();

  // auto forgets the entry: the wire reports auto as absent.
  await setChannelMode(API, { transport: "local", device_id: "alsa#mono", mode: "auto" });
  expect(
    (await listOutputs(API)).find((output) => output.id === "alsa#mono")?.channel_mode,
  ).toBeFalsy();

  // A destination whose channel count the core cannot probe fits stereo:
  // the radio surface is stereo by construction.
  await setChannelMode(API, { transport: "sonos", device_id: "uuid:mock-sonos", mode: "stereo" });
  expect(
    (await listOutputs(API)).find((output) => output.id === "uuid:mock-sonos")?.channel_mode,
  ).toBe("stereo");
  await setChannelMode(API, { transport: "sonos", device_id: "uuid:mock-sonos", mode: "auto" });
});

test("channel mode: open refuses stereo on a mono endpoint too, never collapsing it", {
  tag: "@api",
}, async () => {
  try {
    // A Bluetooth endpoint's real channel count is known only when its sink
    // opens, so the selection is accepted (the core cannot probe it here)...
    await setChannelMode(API, { transport: "bluetooth", device_id: "bt-speaker", mode: "stereo" });
    expect(
      (await listOutputs(API)).find((output) => output.id === "bt-speaker")?.channel_mode,
    ).toBe("stereo");

    // ...and the open-time contract refuses with the same explanation rather
    // than dropping the right channel on the way out.
    const refused = await expectRefusal(
      activateOutput(API, "bluetooth", "bt-speaker"),
      400,
      "invalid_request",
    );
    expect((JSON.parse(refused.body) as ApiErrorBody).error).toBe(
      "Mock Bluetooth Speaker plays mono only; stereo would drop the right channel. Choose Auto or Mono.",
    );

    // Mono opens the very same endpoint fine.
    await setChannelMode(API, { transport: "bluetooth", device_id: "bt-speaker", mode: "mono" });
    await activateOutput(API, "bluetooth", "bt-speaker");
    expect((await getActiveOutput(API))?.transport).toBe("bluetooth");
  } finally {
    // Restore the default so no later test inherits this output's mode.
    await setChannelMode(API, { transport: "bluetooth", device_id: "bt-speaker", mode: "auto" });
  }
});

test("channel mode: mono is a genuine mono stream; auto is byte-identical", {
  tag: "@api",
}, async () => {
  await activateOutput(API, "sonos", "uuid:mock-sonos");
  // The radio stereo header is the historical behaviour...
  const autoBefore = await readWavHeader(API);
  expect(autoBefore.channels).toBe(2);
  expect(autoBefore.blockAlign).toBe(4);
  expect(autoBefore.byteRate).toBe(autoBefore.sampleRate * 4);

  // ...and mono is a TRUE downmix to one sample per frame — a genuine mono
  // stream, not a duplicated mono pair (bytes per frame halve).
  await setChannelMode(API, { transport: "sonos", device_id: "uuid:mock-sonos", mode: "mono" });
  const mono = await readWavHeader(API);
  expect(mono.channels).toBe(1);
  expect(mono.blockAlign).toBe(2);
  expect(mono.byteRate).toBe(mono.sampleRate * 2);

  // auto is byte-identical to what the stream served before any mode was
  // chosen: the historical stereo radio, unchanged.
  await setChannelMode(API, { transport: "sonos", device_id: "uuid:mock-sonos", mode: "auto" });
  const autoAfter = await readWavHeader(API);
  expect(autoAfter.channels).toBe(2);
  expect(Array.from(autoAfter.bytes)).toEqual(Array.from(autoBefore.bytes));
});

test("stereo meter: one LevelMeter reading per channel reaches the client", {
  tag: "@api",
}, async () => {
  const events: WsEvent[] = [];
  const { promise: open, resolve: opened } = Promise.withResolvers<void>();
  const subscription = subscribeEvents(API, undefined, {
    onEvent: (event) => events.push(event),
    onOpen: () => opened(),
  });
  try {
    await open;
    // A live capture feeds the processing thread; the client sees the
    // per-channel meter over its own WebSocket subscription.
    await activateInput(API, "Mock Monitor");
    const meters = () =>
      events.filter(
        (event): event is Extract<WsEvent, { type: "LevelMeter" }> => event.type === "LevelMeter",
      );
    await expect.poll(() => meters().length, { timeout: 15_000 }).toBeGreaterThan(3);
    for (const event of meters()) {
      // The per-channel shape: one reading per channel of the stereo
      // stream, RMS and peak each a full-scale fraction in [0, 1].
      for (const channel of [event.left, event.right]) {
        expect(Number.isFinite(channel.rms)).toBe(true);
        expect(Number.isFinite(channel.peak)).toBe(true);
        expect(channel.rms).toBeGreaterThanOrEqual(0);
        expect(channel.peak).toBeLessThanOrEqual(1);
        expect(channel.rms).toBeLessThanOrEqual(channel.peak);
      }
    }
    // The mock capture is silence by design (mock mode never invents audio
    // content): honest zero levels on both channels.
    for (const event of meters()) {
      expect(event.left.peak).toBe(0);
      expect(event.right.peak).toBe(0);
    }
  } finally {
    subscription.close();
  }
});

test("settings survive a restart: quality and the tuned-once signal", { tag: "@api" }, async () => {
  const configDir = mkdtempSync(path.join(tmpdir(), "on-air-restart-"));
  let verdict: string | undefined;
  const first: MockCore = await startMockCore({
    port: ISOLATED_PORT,
    env: { ON_AIR_CONFIG_DIR: configDir },
  });
  try {
    await setAudioQuality(first.base, "high");
    const report = await runAudioBenchmark(first.base);
    verdict = report.recommended_quality;
    expect((await getTuning(first.base)).tuned).toBe(true);
    // The settings writer debounces writes; let the last one land before
    // the stop, exactly as a user would leave the machine.
    const { promise: settle, resolve: settled } = Promise.withResolvers<void>();
    setTimeout(settled, 500);
    await settle;
  } finally {
    await first.stop();
  }
  // `cargo run` can take a moment to free the port with its binary; wait
  // for it, so the second boot cannot race the first one's shutdown.
  const firstBase = `http://127.0.0.1:${ISOLATED_PORT}`;
  for (;;) {
    const stillUp = await fetch(`${firstBase}/api/status`, { signal: AbortSignal.timeout(500) })
      .then(() => true)
      .catch(() => false);
    if (!stillUp) break;
    const { promise: pause, resolve: paused } = Promise.withResolvers<void>();
    setTimeout(paused, 250);
    await pause;
  }

  const second: MockCore = await startMockCore({
    port: ISOLATED_PORT,
    env: { ON_AIR_CONFIG_DIR: configDir },
  });
  try {
    // The choice survived the restart...
    expect((await getAudioQuality(second.base)).quality).toBe("high");
    // ...and so did the once-per-machine signal: the second boot is no
    // longer a first run, so first-use tuning can never fire again here.
    const tuning = await getTuning(second.base);
    expect(tuning.tuned).toBe(true);
    expect(tuning.recommended_quality).toBe(verdict);
    expect(tuning.quality_override).toBe("high");
    expect(tuning.automatic).toBeNull();
  } finally {
    await second.stop();
  }
});
