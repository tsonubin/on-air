import type { ActiveOutputView } from "@on-air/api-types";
import { describe, expect, it } from "vitest";
import { lampState } from "./Header";

const kitchen: ActiveOutputView = {
  transport: "sonos",
  device_id: "uuid:mock-sonos",
  device_name: "Kitchen",
};

describe("lampState", () => {
  it("is live only when the active output is live (or the core omits the phase)", () => {
    expect(lampState("ok", null).mode).toBe("ok");
    expect(lampState("ok", kitchen).mode).toBe("live");
    expect(lampState("ok", { ...kitchen, state: "live" }).mode).toBe("live");
  });

  it("does not show a starting output as on air", () => {
    const lamp = lampState("ok", { ...kitchen, state: "starting" });
    expect(lamp.mode).toBe("ok");
    expect(lamp.label).toMatch(/connecting/);
  });

  it("warns when the active output failed", () => {
    const lamp = lampState("ok", { ...kitchen, state: "failed" });
    expect(lamp.mode).toBe("warn");
    expect(lamp.label).toMatch(/failed/);
  });
});
