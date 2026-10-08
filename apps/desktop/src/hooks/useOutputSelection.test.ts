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
});
