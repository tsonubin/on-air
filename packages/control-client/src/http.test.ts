import assert from "node:assert/strict";
import { test } from "node:test";
import { activateOutput, apiBase, prettyInput, verifyPin, wsUrl } from "./http.ts";

test("apiBase strips scheme and trailing slash", () => {
  assert.equal(apiBase("192.168.5.14"), "http://192.168.5.14:47990");
  assert.equal(apiBase("http://192.168.5.14/", 48000), "http://192.168.5.14:48000");
});

test("prettyInput matches desktop labels", () => {
  assert.equal(prettyInput("Discard all samples (playback)"), "Null device");
  assert.equal(prettyInput("alsa_output.pci.analog-stereo.monitor"), "Analog monitor");
  assert.equal(prettyInput("PipeWire Sound Server"), "PipeWire");
});

test("wsUrl puts the pairing token on the query string", () => {
  assert.equal(wsUrl("http://192.168.5.14:47990", "onair-1"), "ws://192.168.5.14:47990/api/ws?token=onair-1");
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
    assert.match(String(init?.headers && (init.headers as Record<string, string>).authorization), /Bearer tok/);
    assert.equal(init?.body, JSON.stringify({ transport: "sonos", device_id: "uuid:x" }));
    return new Response(null, { status: 204 });
  };
  await activateOutput("http://10.0.0.2:47990", "sonos", "uuid:x", "tok", fetchImpl);
});
