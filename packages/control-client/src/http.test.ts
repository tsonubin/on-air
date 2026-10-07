import assert from "node:assert/strict";
import { test } from "node:test";
import { prettyInput } from "./deprecated.ts";
import {
  activateInput,
  activateOutput,
  apiBase,
  connectBluetooth,
  controlCd,
  deactivateOutput,
  fetchStatus,
  getActiveInput,
  getActiveOutput,
  getAirplayMode,
  getCd,
  getEq,
  getPairingPin,
  getSampleRate,
  getVolume,
  HttpError,
  listBluetooth,
  listInputs,
  listOutputs,
  openBluetoothSettings,
  pairAirplay,
  pairBluetooth,
  REQUEST_TIMEOUT_MS,
  SLOW_REQUEST_TIMEOUT_MS,
  setEq,
  setSampleRate,
  setVolume,
  simulateCd,
  verifyPin,
  wsUrl,
} from "./http.ts";
import { goldenPathSonos, switchTransports } from "./scenarios.ts";

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

test("prettyInput matches desktop labels", () => {
  assert.equal(prettyInput("Discard all samples (playback)"), "Null device");
  assert.equal(prettyInput("alsa_output.pci.analog-stereo.monitor"), "Analog monitor");
  assert.equal(prettyInput("PipeWire Sound Server"), "PipeWire");
  assert.equal(prettyInput("Audio CD"), "Audio CD");
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

test("deprecated goldenPathSonos still pairs, picks an input, goes live, sets volume and EQ", async () => {
  const calls: string[] = [];
  const fetchImpl: typeof fetch = async (input, init) => {
    const url = String(input);
    calls.push(`${init?.method ?? "GET"} ${url}`);
    if (url.endsWith("/api/status")) return json({ status: "ok", version: "0.1.0" });
    if (url.endsWith("/api/pairing/verify")) return json({ token: "onair-test" });
    if (url.endsWith("/api/inputs")) return json({ inputs: ["Mock Monitor"] });
    if (url.endsWith("/api/outputs")) {
      return json({
        outputs: [{ id: "uuid:mock-sonos", name: "Mock Sonos", transport: "sonos" }],
      });
    }
    return new Response(null, { status: 204 });
  };
  await goldenPathSonos("http://127.0.0.1:47990", "123456", fetchImpl);
  assert.deepEqual(calls, [
    "GET http://127.0.0.1:47990/api/status",
    "POST http://127.0.0.1:47990/api/pairing/verify",
    "GET http://127.0.0.1:47990/api/inputs",
    "POST http://127.0.0.1:47990/api/inputs/active",
    "GET http://127.0.0.1:47990/api/outputs",
    "POST http://127.0.0.1:47990/api/outputs/active",
    "POST http://127.0.0.1:47990/api/outputs/active/volume",
    "PUT http://127.0.0.1:47990/api/eq",
  ]);
});

test("deprecated switchTransports activates Sonos then AirPlay then Bluetooth", async () => {
  const activated: string[] = [];
  const fetchImpl: typeof fetch = async (input, init) => {
    const url = String(input);
    if (url.endsWith("/api/outputs") && !init?.method) {
      return json({
        outputs: [
          { id: "s", name: "S", transport: "sonos" },
          { id: "a", name: "A", transport: "airplay" },
          { id: "b", name: "B", transport: "bluetooth" },
        ],
      });
    }
    if (url.endsWith("/api/outputs/active")) {
      activated.push(JSON.parse(String(init?.body)).transport);
      return new Response(null, { status: 204 });
    }
    throw new Error(`unexpected ${url}`);
  };
  const order = await switchTransports(BASE, "tok", fetchImpl);
  assert.deepEqual(order, ["sonos", "airplay", "bluetooth"]);
  assert.deepEqual(activated, ["sonos", "airplay", "bluetooth"]);
});
