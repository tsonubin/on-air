import assert from "node:assert/strict";
import { test } from "node:test";
import {
  activateOutput,
  apiBase,
  controlCd,
  fetchStatus,
  getVolume,
  goldenPathSonos,
  openBluetoothSettings,
  pairAirplay,
  pairBluetooth,
  prettyInput,
  REQUEST_TIMEOUT_MS,
  setEq,
  setSampleRate,
  setVolume,
  switchTransports,
  verifyPin,
  wsUrl,
} from "./http.ts";

test("apiBase strips scheme and trailing slash", () => {
  assert.equal(apiBase("192.168.5.14"), "http://192.168.5.14:47990");
  assert.equal(apiBase("http://192.168.5.14/", 48000), "http://192.168.5.14:48000");
});

test("requests carry a bounded abort signal", async () => {
  assert.equal(REQUEST_TIMEOUT_MS, 5_000);
  const fetchImpl: typeof fetch = async (_input, init) => {
    assert.ok(init?.signal);
    assert.equal(init.signal.aborted, false);
    return new Response(JSON.stringify({ status: "ok", version: "0.1.0" }), { status: 200 });
  };
  await fetchStatus("http://127.0.0.1:47990", fetchImpl);
});

test("prettyInput matches desktop labels", () => {
  assert.equal(prettyInput("Discard all samples (playback)"), "Null device");
  assert.equal(prettyInput("alsa_output.pci.analog-stereo.monitor"), "Analog monitor");
  assert.equal(prettyInput("PipeWire Sound Server"), "PipeWire");
  assert.equal(prettyInput("Audio CD"), "Audio CD");
});

test("controlCd posts a transport action", async () => {
  const fetchImpl: typeof fetch = async (input, init) => {
    assert.equal(String(input), "http://127.0.0.1:47990/api/cd/control");
    assert.equal(init?.method, "POST");
    assert.equal(init?.body, JSON.stringify({ action: "next" }));
    return new Response(
      JSON.stringify({
        present: true,
        playing: true,
        track: 2,
        track_count: 12,
        title: "Freddie Freeloader",
        position_ms: 0,
        duration_ms: 180000,
      }),
      { status: 200 },
    );
  };
  const status = await controlCd("http://127.0.0.1:47990", "next", undefined, fetchImpl);
  assert.equal(status.track, 2);
});

test("wsUrl puts the pairing token on the query string", () => {
  assert.equal(
    wsUrl("http://192.168.5.14:47990", "onair-1"),
    "ws://192.168.5.14:47990/api/ws?token=onair-1",
  );
});

test("verifyPin posts JSON and returns the token", async () => {
  const fetchImpl: typeof fetch = async (input, init) => {
    assert.equal(String(input), "http://127.0.0.1:47990/api/pairing/verify");
    assert.equal(init?.method, "POST");
    assert.equal(init?.body, JSON.stringify({ pin: "123456" }));
    return new Response(JSON.stringify({ token: "onair-test" }), { status: 200 });
  };
  const token = await verifyPin("http://127.0.0.1:47990", "123456", fetchImpl);
  assert.equal(token, "onair-test");
});

test("activateOutput posts transport and device_id with bearer token", async () => {
  const fetchImpl: typeof fetch = async (input, init) => {
    assert.equal(String(input), "http://10.0.0.2:47990/api/outputs/active");
    assert.match(
      String(init?.headers && (init.headers as Record<string, string>).authorization),
      /Bearer tok/,
    );
    assert.equal(init?.body, JSON.stringify({ transport: "sonos", device_id: "uuid:x" }));
    return new Response(null, { status: 204 });
  };
  await activateOutput("http://10.0.0.2:47990", "sonos", "uuid:x", "tok", fetchImpl);
});

function recordFetch(expected: { url: string; method?: string; body?: string; auth?: string }) {
  const fetchImpl: typeof fetch = async (input, init) => {
    assert.equal(String(input), expected.url);
    if (expected.method) assert.equal(init?.method, expected.method);
    if (expected.body) assert.equal(init?.body, expected.body);
    if (expected.auth) {
      const headers = init?.headers as Record<string, string> | undefined;
      assert.equal(headers?.authorization, expected.auth);
    }
    return new Response(null, { status: 204 });
  };
  return fetchImpl;
}

test("setVolume posts the fader value with the pairing token", async () => {
  await setVolume(
    "http://10.0.0.2:47990",
    20,
    "tok",
    recordFetch({
      url: "http://10.0.0.2:47990/api/outputs/active/volume",
      method: "POST",
      body: JSON.stringify({ volume: 20 }),
      auth: "Bearer tok",
    }),
  );
});

test("getVolume reads the service-owned volume setting", async () => {
  const fetchImpl: typeof fetch = async (input, init) => {
    assert.equal(String(input), "http://10.0.0.2:47990/api/outputs/active/volume");
    const headers = init?.headers as Record<string, string> | undefined;
    assert.equal(headers?.authorization, "Bearer tok");
    return new Response(JSON.stringify({ volume: 37 }), { status: 200 });
  };
  assert.equal(await getVolume("http://10.0.0.2:47990", "tok", fetchImpl), 37);
});

test("setEq puts five-band gains", async () => {
  await setEq(
    "http://10.0.0.2:47990",
    [3, 0, 0, 0, -3],
    "tok",
    recordFetch({
      url: "http://10.0.0.2:47990/api/eq",
      method: "PUT",
      body: JSON.stringify({ gains_db: [3, 0, 0, 0, -3] }),
    }),
  );
});

test("setSampleRate can set input and output independently", async () => {
  await setSampleRate(
    "http://10.0.0.2:47990",
    { input_hz: 48000, output_hz: 44100 },
    "tok",
    recordFetch({
      url: "http://10.0.0.2:47990/api/sample-rate",
      method: "PUT",
      body: JSON.stringify({ input_hz: 48000, output_hz: 44100 }),
    }),
  );
});

test("pairAirplay and pairBluetooth post the device handshake", async () => {
  await pairAirplay(
    "http://10.0.0.2:47990",
    "ap-living",
    "1111",
    "tok",
    recordFetch({
      url: "http://10.0.0.2:47990/api/airplay/pair",
      method: "POST",
      body: JSON.stringify({ device_id: "ap-living", pin: "1111" }),
    }),
  );
  await pairBluetooth(
    "http://10.0.0.2:47990",
    "bt-speaker",
    "tok",
    recordFetch({
      url: "http://10.0.0.2:47990/api/bluetooth/pair",
      method: "POST",
      body: JSON.stringify({ id: "bt-speaker" }),
    }),
  );
  await openBluetoothSettings(
    "http://10.0.0.2:47990",
    "tok",
    recordFetch({
      url: "http://10.0.0.2:47990/api/bluetooth/settings",
      method: "POST",
    }),
  );
});

test("goldenPathSonos pairs, picks an input, goes live on Sonos, sets volume and EQ", async () => {
  const calls: string[] = [];
  const fetchImpl: typeof fetch = async (input, init) => {
    const url = String(input);
    calls.push(`${init?.method ?? "GET"} ${url}`);
    if (url.endsWith("/api/status")) {
      return new Response(JSON.stringify({ status: "ok", version: "0.1.0" }), { status: 200 });
    }
    if (url.endsWith("/api/pairing/verify")) {
      return new Response(JSON.stringify({ token: "onair-test" }), { status: 200 });
    }
    if (url.endsWith("/api/inputs")) {
      return new Response(JSON.stringify({ inputs: ["Mock Monitor"] }), { status: 200 });
    }
    if (url.endsWith("/api/outputs")) {
      return new Response(
        JSON.stringify({
          outputs: [{ id: "uuid:mock-sonos", name: "Mock Sonos", transport: "sonos" }],
        }),
        { status: 200 },
      );
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

test("switchTransports activates Sonos then AirPlay then Bluetooth", async () => {
  const activated: string[] = [];
  const fetchImpl: typeof fetch = async (input, init) => {
    const url = String(input);
    if (url.endsWith("/api/outputs") && !init?.method) {
      return new Response(
        JSON.stringify({
          outputs: [
            { id: "s", name: "S", transport: "sonos" },
            { id: "a", name: "A", transport: "airplay" },
            { id: "b", name: "B", transport: "bluetooth" },
          ],
        }),
        { status: 200 },
      );
    }
    if (url.endsWith("/api/outputs/active")) {
      activated.push(JSON.parse(String(init?.body)).transport);
      return new Response(null, { status: 204 });
    }
    throw new Error(`unexpected ${url}`);
  };
  const order = await switchTransports("http://10.0.0.2:47990", "tok", fetchImpl);
  assert.deepEqual(order, ["sonos", "airplay", "bluetooth"]);
  assert.deepEqual(activated, ["sonos", "airplay", "bluetooth"]);
});
