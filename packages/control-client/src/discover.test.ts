import assert from "node:assert/strict";
import { test } from "node:test";
import { discoverOnAir, isLanUnicast, probeOnAir, subnetHosts } from "./discover.ts";

test("isLanUnicast accepts RFC1918 and rejects loopback and public IPs", () => {
  assert.equal(isLanUnicast("10.0.0.5"), true);
  assert.equal(isLanUnicast("172.16.1.1"), true);
  assert.equal(isLanUnicast("192.168.5.101"), true);
  assert.equal(isLanUnicast("127.0.0.1"), false);
  assert.equal(isLanUnicast("8.8.8.8"), false);
  assert.equal(isLanUnicast("169.254.1.1"), false);
});

test("isLanUnicast bounds every octet to 0-255 and rejects malformed input", () => {
  assert.equal(isLanUnicast("10.0.0.256"), false);
  assert.equal(isLanUnicast("10.256.0.1"), false);
  assert.equal(isLanUnicast("192.168.1.999"), false);
  assert.equal(isLanUnicast("10.0.0.-1"), false);
  assert.equal(isLanUnicast("10.0.0"), false);
  assert.equal(isLanUnicast("10.0.0.1.2"), false);
  assert.equal(isLanUnicast("10.0.0.x"), false);
  assert.equal(isLanUnicast("10.0..1"), false);
  assert.equal(isLanUnicast("10.0.0.1e0"), false);
  assert.equal(isLanUnicast("10.0.0.255"), true);
  assert.equal(isLanUnicast("172.31.255.255"), true);
  assert.equal(isLanUnicast("172.32.0.1"), false);
});

test("subnetHosts expands a /24 from the local IPv4", () => {
  const hosts = subnetHosts("192.168.5.101");
  assert.equal(hosts.length, 254);
  assert.equal(hosts[0], "192.168.5.1");
  assert.equal(hosts[253], "192.168.5.254");
});

test("subnetHosts does not expand loopback into a /24", () => {
  assert.deepEqual(subnetHosts("127.0.0.1"), []);
});

test("probeOnAir returns a host when /api/status is ok", async () => {
  const fetchImpl: typeof fetch = async (input) => {
    assert.equal(String(input), "http://192.168.5.14:47990/api/status");
    return new Response(JSON.stringify({ status: "ok", version: "0.1.0" }), { status: 200 });
  };
  const hit = await probeOnAir("192.168.5.14", 47990, 400, fetchImpl);
  assert.deepEqual(hit, { host: "192.168.5.14", port: 47990, version: "0.1.0", name: "on-air" });
});

test("probeOnAir prefers a name from the status payload over the default", async () => {
  const fetchImpl: typeof fetch = async () =>
    new Response(JSON.stringify({ status: "ok", version: "0.2.0", name: "Studio Mac" }), {
      status: 200,
    });
  const hit = await probeOnAir("192.168.5.14", 47990, 400, fetchImpl);
  assert.equal(hit?.name, "Studio Mac");
  assert.equal(hit?.version, "0.2.0");
});

test("probeOnAir rejects a non-ok status payload", async () => {
  const fetchImpl: typeof fetch = async () =>
    new Response(JSON.stringify({ status: "paused" }), { status: 200 });
  assert.equal(await probeOnAir("192.168.5.14", 47990, 400, fetchImpl), null);
});

test("probeOnAir tolerates Expo Go development overhead on a reachable LAN service", async () => {
  const fetchImpl: typeof fetch = async (_input, init) =>
    await new Promise<Response>((resolve, reject) => {
      const timer = setTimeout(() => {
        resolve(new Response(JSON.stringify({ status: "ok", version: "0.1.0" }), { status: 200 }));
      }, 500);
      init?.signal?.addEventListener(
        "abort",
        () => {
          clearTimeout(timer);
          reject(new DOMException("The operation was aborted", "AbortError"));
        },
        { once: true },
      );
    });

  const hit = await probeOnAir("192.168.5.14", 47990, undefined, fetchImpl);
  assert.equal(hit?.host, "192.168.5.14");
});

test("discoverOnAir dedupes extraHosts and ignores failed probes", async () => {
  const fetchImpl: typeof fetch = async (input) => {
    const url = String(input);
    if (url.includes("192.168.5.14") || url.includes("127.0.0.1")) {
      return new Response(JSON.stringify({ status: "ok", version: "0.1.0" }), { status: 200 });
    }
    return new Response("nope", { status: 500 });
  };
  const found = await discoverOnAir({
    extraHosts: ["192.168.5.14", "192.168.5.14"],
    fetchImpl,
  });
  assert.equal(found.length, 2);
  assert.ok(found.some((h) => h.host === "127.0.0.1"));
  assert.ok(found.some((h) => h.host === "192.168.5.14"));
});

test("discoverOnAir does not expand a loopback address into a /24 scan", async () => {
  const probed: string[] = [];
  const fetchImpl: typeof fetch = async (input) => {
    probed.push(String(input));
    return new Response(JSON.stringify({ status: "ok", version: "0.1.0" }), { status: 200 });
  };
  await discoverOnAir({ localIp: "127.0.0.1", fetchImpl });
  assert.deepEqual(probed, ["http://127.0.0.1:47990/api/status"]);
});

test("LAN discovery stops probing after the first responsive batch", async () => {
  const probed: string[] = [];
  const fetchImpl: typeof fetch = async (input) => {
    const url = String(input);
    probed.push(url);
    if (url.includes("192.168.5.100")) {
      return new Response(JSON.stringify({ status: "ok", version: "0.1.0" }), { status: 200 });
    }
    return new Response("nope", { status: 500 });
  };

  const found = await discoverOnAir({ localIp: "192.168.5.101", fetchImpl });

  assert.ok(found.some((host) => host.host === "192.168.5.100"));
  assert.ok(probed.length < 64, `expected a bounded quick scan, got ${probed.length} probes`);
});
