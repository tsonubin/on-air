import {
  fetchStatus,
  goldenPathSonos,
  switchTransports,
  verifyPin,
} from "../../apps/mobile/src/controlClient.ts";

async function main() {
  const base = process.env.API_BASE ?? "http://127.0.0.1:47990";
  const status = await fetchStatus(base);
  if (status.status !== "ok") {
    throw new Error(`status ${status.status}`);
  }
  const token = await verifyPin(base, "123456");
  if (!token.startsWith("onair-")) {
    throw new Error("bad token");
  }
  await goldenPathSonos(base, "123456");
  const switched = await switchTransports(base);
  if (switched.join(",") !== "sonos,airplay,bluetooth") {
    throw new Error(`switch ${switched}`);
  }
  console.log("mobile controlClient: pairing + sonos golden path + transport switch ok");
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
