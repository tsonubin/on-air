import { HttpError } from "@on-air/control-client";
import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { fakeClient, fakeCore, sonos } from "../test/fakes";
import type { CoreSnapshot } from "./useCoreSnapshot";
import { useOutputSelection } from "./useOutputSelection";

function setup() {
  const client = fakeClient(fakeCore());
  const snapshot = { outputs: [sonos], airplayMode: "owntone" } as CoreSnapshot;
  const core = {
    snapshot,
    refresh: vi.fn(async () => {}),
    reportError: vi.fn(),
    clearError: vi.fn(),
    patch: { cd: vi.fn(), activeOutput: vi.fn(), activeInput: vi.fn() },
  };
  const hook = renderHook(() => useOutputSelection({ core, client }));
  return { client, core, hook };
}

describe("useOutputSelection", () => {
  it("marks a successfully activated output live", async () => {
    const { core, hook } = setup();
    await act(() => hook.result.current.chooseOutput(sonos));
    expect(core.patch.activeOutput).toHaveBeenCalledWith(
      expect.objectContaining({ device_id: sonos.id, state: "live" }),
    );
  });

  it("reads back the output state after a failed activation", async () => {
    const { client, core, hook } = setup();
    const err = new HttpError(
      "/api/outputs/active",
      409,
      JSON.stringify({ error: "not connected", code: "not_ready" }),
    );
    client.activateOutput.mockRejectedValueOnce(err);
    await act(() => hook.result.current.chooseOutput(sonos));
    expect(core.reportError).toHaveBeenCalledWith(err);
    expect(core.patch.activeOutput).not.toHaveBeenCalled();
    expect(core.refresh).toHaveBeenCalledWith("devices");
    expect(hook.result.current.connectingOutput).toBeNull();
  });

  it("sends a manual latency trim and refreshes the sync rows", async () => {
    const { client, core, hook } = setup();
    await act(() => hook.result.current.setTrim(sonos, -100));
    expect(client.setSyncTrim).toHaveBeenCalledWith({
      transport: "sonos",
      device_id: sonos.id,
      trim_ms: -100,
    });
    expect(core.refresh).toHaveBeenCalledWith("devices");
    expect(core.reportError).not.toHaveBeenCalled();
  });

  it("reports a refused trim without leaving a write in flight", async () => {
    const { client, core, hook } = setup();
    const err = new HttpError(
      "/api/sync/trim",
      400,
      JSON.stringify({
        error: "trim_ms must be between -10000 and 10000",
        code: "invalid_request",
      }),
    );
    client.setSyncTrim.mockRejectedValueOnce(err);
    await act(() => hook.result.current.setTrim(sonos, 99_999));
    expect(core.reportError).toHaveBeenCalledWith(err);
    expect(core.refresh).not.toHaveBeenCalled();
  });
});
