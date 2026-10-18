import assert from "node:assert/strict";
import { test } from "node:test";
import {
  activateInput,
  activateOutput,
  addSyncGroupMember,
  apiBase,
  connectBluetooth,
  controlCd,
  createSyncGroup,
  deactivateOutput,
  dissolveSyncGroup,
  fetchStatus,
  getActiveInput,
  getActiveOutput,
  getAirplayMode,
  getAudioQuality,
  getCd,
  getEq,
  getPairingPin,
  getSampleRate,
  getSync,
  getVolume,
  HttpError,
  leaveSyncGroupMember,
  listBluetooth,
  listInputs,
  listOutputs,
  openBluetoothSettings,
  pairAirplay,
  pairBluetooth,
  REQUEST_TIMEOUT_MS,
  SLOW_REQUEST_TIMEOUT_MS,
  setAudioQuality,
  setEq,
  setSampleRate,
  setSyncTrim,
  setVolume,
  simulateCd,
  startSyncGroup,
  stopSyncGroup,
  verifyPin,
  wsUrl,
} from "./http.ts";

const BASE = "http://10.0.0.2:47990";

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

/** A fetch that records the call and answers with `response`. */
function respond(response: Response | (() => Response)) {
  const calls: { url: string; init: RequestInit | undefined }[] = [];
  const fetchImpl: typeof fetch = async (input, init) => {
    calls.push({ url: String(input), init });
    return typeof response === "function" ? response() : response;
  };
  return { fetchImpl, calls };
}

/** A fetch that never answers; rejects with the signal's reason when aborted. */
function hanging(): typeof fetch {
  return (_input, init) =>
    new Promise<Response>((_resolve, reject) => {
      const signal = init?.signal;
      if (!signal) return;
      if (signal.aborted) reject(signal.reason);
      signal.addEventListener("abort", () => reject(signal.reason), { once: true });
    });
}

function headersOf(init: RequestInit | undefined): Record<string, string> {
  return (init?.headers ?? {}) as Record<string, string>;
}

test("apiBase strips scheme and trailing slash", () => {
  assert.equal(apiBase("192.168.5.14"), "http://192.168.5.14:47990");
  assert.equal(apiBase("http://192.168.5.14/", 48000), "http://192.168.5.14:48000");
  assert.equal(apiBase("  HTTPS://host.local// "), "http://host.local:47990");
});

test("apiBase brackets bare IPv6 literals and leaves bracketed ones alone", () => {
  assert.equal(apiBase("fe80::1"), "http://[fe80::1]:47990");
  assert.equal(apiBase("[fe80::1]"), "http://[fe80::1]:47990");
  assert.equal(apiBase("http://[::1]/", 48000), "http://[::1]:48000");
  assert.equal(apiBase("2001:db8::aa:bb"), "http://[2001:db8::aa:bb]:47990");
});

test("timeout defaults are 5 s for reads and 60 s for device handshakes", () => {
  assert.equal(REQUEST_TIMEOUT_MS, 5_000);
  assert.equal(SLOW_REQUEST_TIMEOUT_MS, 60_000);
});

test("requests carry a bounded abort signal", async () => {
  const { fetchImpl, calls } = respond(json({ status: "ok", version: "0.1.0" }));
  await fetchStatus(BASE, { fetchImpl });
  assert.ok(calls[0].init?.signal);
  assert.equal(calls[0].init?.signal?.aborted, false);
});

test("a request past timeoutMs rejects with a TimeoutError naming the URL", async () => {
  await assert.rejects(fetchStatus(BASE, { fetchImpl: hanging(), timeoutMs: 20 }), (error) => {
    assert.ok(error instanceof Error);
    assert.equal(error.name, "TimeoutError");
    assert.match(error.message, /http:\/\/10\.0\.0\.2:47990\/api\/status timed out after 20 ms/);
    return true;
  });
});

test("a caller abort propagates as the caller's reason", async () => {
  const controller = new AbortController();
  const pending = listInputs(BASE, "tok", { fetchImpl: hanging(), signal: controller.signal });
  const reason = new Error("user navigated away");
  controller.abort(reason);
  await assert.rejects(pending, (error) => error === reason);

  const bare = new AbortController();
  const pendingBare = listInputs(BASE, "tok", { fetchImpl: hanging(), signal: bare.signal });
  bare.abort();
  await assert.rejects(pendingBare, (error) => {
    assert.ok(error instanceof Error);
    assert.equal(error.name, "AbortError");
    return true;
  });
});

test("timeout and caller abort still work without AbortSignal.any (RN/WebKit path)", async (t) => {
  const anyImpl = (AbortSignal as { any?: unknown }).any;
  assert.equal(typeof anyImpl, "function", "Node 22 is expected to have AbortSignal.any");
  delete (AbortSignal as { any?: unknown }).any;
  t.after(() => {
    (AbortSignal as { any?: unknown }).any = anyImpl;
  });

  await assert.rejects(
    fetchStatus(BASE, { fetchImpl: hanging(), timeoutMs: 10 }),
    (error) => error instanceof Error && error.name === "TimeoutError",
  );

  const controller = new AbortController();
  const reason = new Error("cancelled");
  const pending = fetchStatus(BASE, { fetchImpl: hanging(), signal: controller.signal });
  controller.abort(reason);
  await assert.rejects(pending, (error) => error === reason);

  // The fallback listener is detached once the request settles.
  const seen = respond(json({ status: "ok", version: "0.1.0" }));
  const probe = new AbortController();
  await fetchStatus(BASE, { fetchImpl: seen.fetchImpl, signal: probe.signal });
  assert.equal(seen.calls.length, 1);
  assert.equal(seen.calls[0].init?.signal?.aborted, false);
});

test("an already-aborted caller signal rejects before fetching", async () => {
  const controller = new AbortController();
  controller.abort(new Error("stale"));
  // This fetch ignores signals, so the only way to see zero calls is for the
  // client to check the caller signal before dispatching.
  const { fetchImpl, calls } = respond(json({ inputs: [] }));
  await assert.rejects(listInputs(BASE, "tok", { fetchImpl, signal: controller.signal }), /stale/);
  assert.equal(calls.length, 0);
});

test("HttpError carries status, raw body and the envelope code", async () => {
  const { fetchImpl } = respond(
    json({ error: "another client is casting to Kitchen", code: "conflict" }, 409),
  );
  await assert.rejects(activateOutput(BASE, "sonos", "uuid:x", "tok", { fetchImpl }), (error) => {
    assert.ok(error instanceof HttpError);
    assert.equal(error.name, "HttpError");
    assert.equal(error.status, 409);
    assert.equal(error.path, "/api/outputs/active");
    assert.equal(error.code, "conflict");
    assert.equal(error.body, '{"error":"another client is casting to Kitchen","code":"conflict"}');
    assert.match(error.message, /\/api\/outputs\/active 409: another client is casting/);
    return true;
  });
});

test("HttpError keeps a non-envelope body as text with no code", async () => {
  const { fetchImpl } = respond(new Response("no audio compact disc", { status: 409 }));
  await assert.rejects(controlCd(BASE, "play", "tok", undefined, { fetchImpl }), (error) => {
    assert.ok(error instanceof HttpError);
    assert.equal(error.status, 409);
    assert.equal(error.code, undefined);
    assert.equal(error.body, "no audio compact disc");
    assert.equal(error.message, "/api/cd/control 409");
    return true;
  });
  const empty = respond(new Response(null, { status: 503 }));
  await assert.rejects(fetchStatus(BASE, { fetchImpl: empty.fetchImpl }), (error) => {
    assert.ok(error instanceof HttpError);
    assert.equal(error.body, "");
    assert.equal(error.code, undefined);
    return true;
  });
});

test("204 and empty 200 bodies resolve to undefined", async () => {
  const noContent = respond(new Response(null, { status: 204 }));
  assert.equal(await activateInput(BASE, "Mock Monitor", "tok", noContent), undefined);
  const emptyOk = respond(new Response("", { status: 200 }));
  assert.equal(await activateInput(BASE, "Mock Monitor", "tok", emptyOk), undefined);
  assert.equal(emptyOk.calls[0].init?.body, JSON.stringify({ name: "Mock Monitor" }));
});

test("getPairingPin reads the loopback-only PIN", async () => {
  const { fetchImpl, calls } = respond(json({ pin: "123456" }));
  assert.equal(await getPairingPin(BASE, { fetchImpl }), "123456");
  assert.equal(calls[0].url, `${BASE}/api/pairing/pin`);
  assert.equal(calls[0].init?.method, undefined);
});

test("verifyPin posts JSON and returns the token", async () => {
  const { fetchImpl, calls } = respond(json({ token: "onair-test" }));
  assert.equal(await verifyPin(BASE, "123456", { fetchImpl }), "onair-test");
  assert.equal(calls[0].url, `${BASE}/api/pairing/verify`);
  assert.equal(calls[0].init?.method, "POST");
  assert.equal(calls[0].init?.body, JSON.stringify({ pin: "123456" }));
  assert.equal(headersOf(calls[0].init)["content-type"], "application/json");
});

test("listInputs and getActiveInput read the input catalog with the token", async () => {
  const inputs = respond(json({ inputs: ["Audio CD", "Mock Monitor"] }));
  assert.deepEqual(await listInputs(BASE, "tok", inputs), ["Audio CD", "Mock Monitor"]);
  assert.equal(inputs.calls[0].url, `${BASE}/api/inputs`);
  assert.equal(headersOf(inputs.calls[0].init).authorization, "Bearer tok");

  const active = respond(json({ name: "Mock Monitor", backend: "pipewire-monitor" }));
  assert.deepEqual(await getActiveInput(BASE, "tok", active), {
    name: "Mock Monitor",
    backend: "pipewire-monitor",
  });
  assert.equal(active.calls[0].url, `${BASE}/api/inputs/active`);
});

test("listOutputs returns the typed catalog", async () => {
  const { fetchImpl, calls } = respond(
    json({
      outputs: [
        {
          id: "uuid:s",
          name: "Kitchen",
          transport: "sonos",
          kind: "pair",
          member_count: 2,
          needs_pair: false,
          paired: true,
        },
      ],
    }),
  );
  const outputs = await listOutputs(BASE, "tok", { fetchImpl });
  assert.equal(outputs.length, 1);
  assert.equal(outputs[0].kind, "pair");
  assert.equal(outputs[0].member_count, 2);
  assert.equal(calls[0].url, `${BASE}/api/outputs`);
});

test("getActiveOutput unwraps {active} and maps null through", async () => {
  const live = respond(
    json({ active: { transport: "sonos", device_id: "uuid:s", device_name: "Kitchen" } }),
  );
  assert.deepEqual(await getActiveOutput(BASE, "tok", live), {
    transport: "sonos",
    device_id: "uuid:s",
    device_name: "Kitchen",
  });
  assert.equal(live.calls[0].url, `${BASE}/api/outputs/active`);

  const idle = respond(json({ active: null }));
  assert.equal(await getActiveOutput(BASE, "tok", idle), null);
});

test("activateOutput posts transport and device_id with bearer token", async () => {
  const { fetchImpl, calls } = respond(new Response(null, { status: 204 }));
  await activateOutput(BASE, "sonos", "uuid:x", "tok", { fetchImpl });
  assert.equal(calls[0].url, `${BASE}/api/outputs/active`);
  assert.equal(calls[0].init?.method, "POST");
  assert.equal(headersOf(calls[0].init).authorization, "Bearer tok");
  assert.equal(calls[0].init?.body, JSON.stringify({ transport: "sonos", device_id: "uuid:x" }));
});

test("deactivateOutput sends DELETE /api/outputs/active", async () => {
  const { fetchImpl, calls } = respond(new Response(null, { status: 204 }));
  await deactivateOutput(BASE, "tok", { fetchImpl });
  assert.equal(calls[0].url, `${BASE}/api/outputs/active`);
  assert.equal(calls[0].init?.method, "DELETE");
  assert.equal(calls[0].init?.body, undefined);
  assert.equal(headersOf(calls[0].init).authorization, "Bearer tok");
});

test("setVolume posts an integer fader value with the pairing token", async () => {
  const { fetchImpl, calls } = respond(() => new Response(null, { status: 204 }));
  await setVolume(BASE, 20, "tok", { fetchImpl });
  assert.equal(calls[0].url, `${BASE}/api/outputs/active/volume`);
  assert.equal(calls[0].init?.method, "POST");
  assert.equal(calls[0].init?.body, JSON.stringify({ volume: 20 }));
  assert.equal(headersOf(calls[0].init).authorization, "Bearer tok");
  // The core deserialises `volume` as u8; a fractional fader value must not 422.
  await setVolume(BASE, 37.6, "tok", { fetchImpl });
  assert.equal(calls[1].init?.body, JSON.stringify({ volume: 38 }));
});

test("getVolume reads the service-owned volume setting", async () => {
  const { fetchImpl, calls } = respond(json({ volume: 37 }));
  assert.equal(await getVolume(BASE, "tok", { fetchImpl }), 37);
  assert.equal(calls[0].url, `${BASE}/api/outputs/active/volume`);
  assert.equal(headersOf(calls[0].init).authorization, "Bearer tok");
});

test("getEq and setEq round-trip five-band gains", async () => {
  const read = respond(json({ gains_db: [3, 0, 0, 0, -3] }));
  assert.deepEqual(await getEq(BASE, "tok", read), [3, 0, 0, 0, -3]);
  assert.equal(read.calls[0].url, `${BASE}/api/eq`);

  const write = respond(new Response(null, { status: 204 }));
  await setEq(BASE, [3, 0, 0, 0, -3], "tok", write);
  assert.equal(write.calls[0].url, `${BASE}/api/eq`);
  assert.equal(write.calls[0].init?.method, "PUT");
  assert.equal(write.calls[0].init?.body, JSON.stringify({ gains_db: [3, 0, 0, 0, -3] }));
});

test("getSampleRate returns both sides; setSampleRate sets them independently", async () => {
  const read = respond(
    json({
      sample_rate_hz: 48000,
      input: { sample_rate_hz: 48000, supported_hz: [44100, 48000] },
      output: { sample_rate_hz: 44100, supported_hz: [44100], transport: "sonos" },
    }),
  );
  const rates = await getSampleRate(BASE, "tok", read);
  assert.equal(rates.output.transport, "sonos");
  assert.equal(rates.input.transport, undefined);
  assert.equal(read.calls[0].url, `${BASE}/api/sample-rate`);

  const write = respond(new Response(null, { status: 204 }));
  await setSampleRate(BASE, { input_hz: 48000, output_hz: 44100 }, "tok", write);
  assert.equal(write.calls[0].init?.method, "PUT");
  assert.equal(write.calls[0].init?.body, JSON.stringify({ input_hz: 48000, output_hz: 44100 }));
});

test("getAirplayMode returns the platform strategy", async () => {
  const { fetchImpl, calls } = respond(json({ mode: "owntone" }));
  assert.equal(await getAirplayMode(BASE, "tok", { fetchImpl }), "owntone");
  assert.equal(calls[0].url, `${BASE}/api/airplay/mode`);
});

test("pairAirplay and pairBluetooth post the device handshake", async () => {
  const airplay = respond(new Response(null, { status: 204 }));
  await pairAirplay(BASE, "ap-living", "1111", "tok", airplay);
  assert.equal(airplay.calls[0].url, `${BASE}/api/airplay/pair`);
  assert.equal(airplay.calls[0].init?.method, "POST");
  assert.equal(
    airplay.calls[0].init?.body,
    JSON.stringify({ device_id: "ap-living", pin: "1111" }),
  );

  const bluetooth = respond(new Response(null, { status: 204 }));
  await pairBluetooth(BASE, "bt-speaker", "tok", bluetooth);
  assert.equal(bluetooth.calls[0].url, `${BASE}/api/bluetooth/pair`);
  assert.equal(bluetooth.calls[0].init?.body, JSON.stringify({ id: "bt-speaker" }));

  const settings = respond(new Response(null, { status: 204 }));
  await openBluetoothSettings(BASE, "tok", settings);
  assert.equal(settings.calls[0].url, `${BASE}/api/bluetooth/settings`);
  assert.equal(settings.calls[0].init?.method, "POST");
  assert.equal(settings.calls[0].init?.body, undefined);
});

test("connectBluetooth posts the id and listBluetooth reads the typed list", async () => {
  const connect = respond(new Response(null, { status: 204 }));
  await connectBluetooth(BASE, "bt-speaker", "tok", connect);
  assert.equal(connect.calls[0].url, `${BASE}/api/bluetooth/connect`);
  assert.equal(connect.calls[0].init?.method, "POST");
  assert.equal(connect.calls[0].init?.body, JSON.stringify({ id: "bt-speaker" }));

  const list = respond(
    json({ devices: [{ id: "bt-speaker", name: "Speaker", paired: true, connected: false }] }),
  );
  const devices = await listBluetooth(BASE, "tok", list);
  assert.equal(list.calls[0].url, `${BASE}/api/bluetooth/devices`);
  assert.equal(devices.length, 1);
  assert.equal(devices[0].paired, true);
});

test("slow device calls accept a caller timeout override", async () => {
  await assert.rejects(
    connectBluetooth(BASE, "bt-speaker", "tok", { fetchImpl: hanging(), timeoutMs: 10 }),
    (error) => error instanceof Error && error.name === "TimeoutError",
  );
});

test("wsUrl puts the pairing token on the query string", () => {
  assert.equal(
    wsUrl("http://192.168.5.14:47990", "onair-1"),
    "ws://192.168.5.14:47990/api/ws?token=onair-1",
  );
  assert.equal(wsUrl("http://192.168.5.14:47990"), "ws://192.168.5.14:47990/api/ws");
});

test("getCd reads the disc status", async () => {
  const { fetchImpl, calls } = respond(
    json({
      present: true,
      playing: false,
      track: 1,
      track_count: 2,
      album: "Kind of Blue",
      position_ms: 0,
      duration_ms: 1000,
      tracks: [
        { number: 1, duration_ms: 500 },
        { number: 2, title: "So What", duration_ms: 500 },
      ],
    }),
  );
  const status = await getCd(BASE, "tok", { fetchImpl });
  assert.equal(status.tracks.length, 2);
  assert.equal(status.tracks[0].title, undefined);
  assert.equal(calls[0].url, `${BASE}/api/cd`);
});

test("controlCd posts a transport action with optional seek/goto fields", async () => {
  const { fetchImpl, calls } = respond(() =>
    json({
      present: true,
      playing: true,
      track: 2,
      track_count: 12,
      title: "Freddie Freeloader",
      position_ms: 0,
      duration_ms: 180000,
      tracks: [],
    }),
  );
  const status = await controlCd(BASE, "next", undefined, undefined, { fetchImpl });
  assert.equal(status.track, 2);
  assert.equal(calls[0].url, `${BASE}/api/cd/control`);
  assert.equal(calls[0].init?.method, "POST");
  assert.equal(calls[0].init?.body, JSON.stringify({ action: "next" }));

  await controlCd(BASE, "seek", "tok", { position_ms: 1500 }, { fetchImpl });
  assert.equal(calls[1].init?.body, JSON.stringify({ action: "seek", position_ms: 1500 }));
});

test("simulateCd posts to the mock-only /api/mock/cd route", async () => {
  const { fetchImpl, calls } = respond(
    json({
      present: true,
      playing: false,
      track: 1,
      track_count: 1,
      album: "Test",
      position_ms: 0,
      duration_ms: 1000,
      tracks: [{ number: 1, duration_ms: 1000 }],
    }),
  );
  const body = { present: true, album: "Test", tracks: [{ duration_ms: 1000 }] };
  const status = await simulateCd(BASE, body, "tok", { fetchImpl });
  assert.equal(status.present, true);
  assert.equal(calls[0].url, `${BASE}/api/mock/cd`);
  assert.equal(calls[0].init?.method, "POST");
  assert.equal(calls[0].init?.body, JSON.stringify(body));
});

test("getAudioQuality reads the DSP profile selection", async () => {
  const { fetchImpl, calls } = respond(
    json({ quality: "balanced", recommended: "high", profiles: ["economy", "balanced", "high"] }),
  );
  assert.deepEqual(await getAudioQuality(BASE, "tok", { fetchImpl }), {
    quality: "balanced",
    recommended: "high",
    profiles: ["economy", "balanced", "high"],
  });
  assert.equal(calls[0].url, `${BASE}/api/audio/quality`);
  assert.equal(headersOf(calls[0].init).authorization, "Bearer tok");
});

test("setAudioQuality puts one profile and accepts the empty 204", async () => {
  const { fetchImpl, calls } = respond(new Response(null, { status: 204 }));
  assert.equal(await setAudioQuality(BASE, "high", "tok", { fetchImpl }), undefined);
  assert.equal(calls[0].url, `${BASE}/api/audio/quality`);
  assert.equal(calls[0].init?.method, "PUT");
  assert.equal(calls[0].init?.body, JSON.stringify({ quality: "high" }));
});

test("getSync reads every output's latency and sync state", async () => {
  const { fetchImpl, calls } = respond(
    json({
      outputs: [
        {
          transport: "bluetooth",
          device_id: "bt-1",
          device_name: "Patio",
          state: "locked",
          trim_ms: -100,
          default_latency_ms: 150,
          effective_latency_ms: 50,
          calibrated: false,
          delay_frames: 2400,
          output_sample_rate_hz: 48000,
          drift_ppm: 42,
        },
      ],
    }),
  );
  const body = await getSync(BASE, "tok", { fetchImpl });
  assert.equal(body.outputs.length, 1);
  assert.equal(body.outputs[0].state, "locked");
  assert.equal(body.outputs[0].trim_ms, -100);
  assert.equal(body.outputs[0].drift_ppm, 42);
  assert.equal(calls[0].url, `${BASE}/api/sync`);
});

test("setSyncTrim puts a manual trim and returns its effective latency", async () => {
  const { fetchImpl, calls } = respond(json({ trim_ms: -100, effective_latency_ms: 50 }));
  const result = await setSyncTrim(
    BASE,
    { transport: "bluetooth", device_id: "bt-1", trim_ms: -100 },
    "tok",
    { fetchImpl },
  );
  assert.deepEqual(result, { trim_ms: -100, effective_latency_ms: 50 });
  assert.equal(calls[0].url, `${BASE}/api/sync/trim`);
  assert.equal(calls[0].init?.method, "PUT");
  assert.equal(
    calls[0].init?.body,
    JSON.stringify({ transport: "bluetooth", device_id: "bt-1", trim_ms: -100 }),
  );
});

test("the sync-group routes drive the whole group lifecycle", async () => {
  // Create returns the new group so the caller has its id.
  const created = respond(json({ id: 7, name: "Downstairs", members: [], playing: false }));
  const group = await createSyncGroup(BASE, "Downstairs", "tok", { fetchImpl: created.fetchImpl });
  assert.equal(group.id, 7);
  assert.equal(created.calls[0].url, `${BASE}/api/sync/groups`);
  assert.equal(created.calls[0].init?.method, "POST");
  assert.equal(created.calls[0].init?.body, JSON.stringify({ name: "Downstairs" }));

  // Membership is a POST per move: add to one group, take out again.
  const added = respond(new Response(null, { status: 204 }));
  await addSyncGroupMember(BASE, 7, { transport: "sonos", device_id: "uuid:1" }, "tok", {
    fetchImpl: added.fetchImpl,
  });
  assert.equal(added.calls[0].url, `${BASE}/api/sync/groups/7/members`);
  assert.equal(added.calls[0].init?.method, "POST");
  assert.equal(
    added.calls[0].init?.body,
    JSON.stringify({ transport: "sonos", device_id: "uuid:1" }),
  );

  const left = respond(new Response(null, { status: 204 }));
  await leaveSyncGroupMember(BASE, 7, { transport: "sonos", device_id: "uuid:1" }, "tok", {
    fetchImpl: left.fetchImpl,
  });
  assert.equal(left.calls[0].url, `${BASE}/api/sync/groups/7/leave`);
  assert.equal(left.calls[0].init?.method, "POST");

  // Playback: start together at one epoch, stop, dissolve.
  const started = respond(new Response(null, { status: 204 }));
  await startSyncGroup(BASE, 7, "tok", { fetchImpl: started.fetchImpl });
  assert.equal(started.calls[0].url, `${BASE}/api/sync/groups/7/start`);
  assert.equal(started.calls[0].init?.method, "POST");

  const stopped = respond(new Response(null, { status: 204 }));
  await stopSyncGroup(BASE, 7, "tok", { fetchImpl: stopped.fetchImpl });
  assert.equal(stopped.calls[0].url, `${BASE}/api/sync/groups/7/stop`);
  assert.equal(stopped.calls[0].init?.method, "POST");

  const dissolved = respond(new Response(null, { status: 204 }));
  await dissolveSyncGroup(BASE, 7, "tok", { fetchImpl: dissolved.fetchImpl });
  assert.equal(dissolved.calls[0].url, `${BASE}/api/sync/groups/7`);
  assert.equal(dissolved.calls[0].init?.method, "DELETE");
});
