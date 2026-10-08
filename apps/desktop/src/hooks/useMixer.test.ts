import type { EqGains } from "@on-air/api-types";
import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { deferred, fakeClient, fakeCore, sampleRate, tick } from "../test/fakes";
import type { CoreSnapshot } from "./useCoreSnapshot";
import { useMixer } from "./useMixer";

function snapshotWith(partial: Partial<CoreSnapshot>): CoreSnapshot {
  return {
    status: null,
    inputs: null,
    activeInput: null,
    outputs: null,
    activeOutput: null,
    volume: 50,
    gains: [0, 0, 0, 0, 0],
    sampleRate,
    pin: null,
    airplayMode: null,
    cd: null,
    ...partial,
  };
}

function setup(initial: CoreSnapshot = snapshotWith({})) {
  const client = fakeClient(fakeCore());
  const reportError = vi.fn();
  const refresh = vi.fn(async () => {});
  const hook = renderHook(({ snapshot }) => useMixer({ snapshot, client, reportError, refresh }), {
    initialProps: { snapshot: initial },
  });
  return { client, reportError, refresh, hook };
}

describe("useMixer", () => {
  it("seeds controls from the snapshot", () => {
    const { hook } = setup();
    expect(hook.result.current.volume).toBe(50);
    expect(hook.result.current.gains).toEqual([0, 0, 0, 0, 0]);
    expect(hook.result.current.inputHz).toBe(44100);
    expect(hook.result.current.outputHz).toBe(48000);
    expect(hook.result.current.outputRates).toEqual([44100, 48000, 96000]);
  });

  it("keeps only the newest pending volume while a write is in flight", async () => {
    const { client, hook } = setup();
    const first = deferred();
    client.setVolume.mockReturnValueOnce(first.promise);
    act(() => hook.result.current.setVolume(10));
    act(() => hook.result.current.setVolume(20));
    act(() => hook.result.current.setVolume(30));
    expect(hook.result.current.volume).toBe(30);
    expect(client.setVolume).toHaveBeenCalledTimes(1);
    first.resolve();
    await waitFor(() => expect(client.setVolume).toHaveBeenCalledTimes(2));
    expect(client.setVolume.mock.calls.map((c) => c[0])).toEqual([10, 30]);
    await tick();
    expect(hook.result.current.volume).toBe(30);
  });

  it("ignores a polled value for a key while its write is in flight", async () => {
    const { client, hook } = setup();
    const pending = deferred();
    client.setVolume.mockReturnValueOnce(pending.promise);
    act(() => hook.result.current.setVolume(30));
    hook.rerender({ snapshot: snapshotWith({ volume: 77 }) });
    expect(hook.result.current.volume).toBe(30);
    pending.resolve();
    await tick();
    expect(hook.result.current.volume).toBe(30);
    // A later poll that reflects the write is accepted again.
    hook.rerender({ snapshot: snapshotWith({ volume: 30 }) });
    hook.rerender({ snapshot: snapshotWith({ volume: 31 }) });
    expect(hook.result.current.volume).toBe(31);
  });

  it("rolls back only the band whose write failed", async () => {
    const { client, hook, reportError } = setup();
    act(() => hook.result.current.setGain(1, 4));
    await waitFor(() => expect(client.setEq).toHaveBeenCalledTimes(1));
    client.setEq.mockRejectedValueOnce(new Error("nope"));
    act(() => hook.result.current.setGain(3, -6));
    expect(hook.result.current.gains).toEqual([0, 4, 0, -6, 0]);
    await waitFor(() => expect(reportError).toHaveBeenCalledTimes(1));
    expect(hook.result.current.gains).toEqual([0, 4, 0, 0, 0]);
  });

  it("sends the latest gains once after a burst and uses functional updates", async () => {
    const { client, hook } = setup();
    const first = deferred();
    client.setEq.mockReturnValueOnce(first.promise);
    act(() => {
      hook.result.current.setGain(0, 1);
      hook.result.current.setGain(1, 2);
      hook.result.current.setGain(2, 3);
    });
    expect(hook.result.current.gains).toEqual([1, 2, 3, 0, 0]);
    expect(client.setEq).toHaveBeenCalledTimes(1);
    first.resolve();
    await waitFor(() => expect(client.setEq).toHaveBeenCalledTimes(2));
    const sent = client.setEq.mock.calls.map((c) => c[0] as EqGains);
    expect(sent[0]).toEqual([1, 0, 0, 0, 0]);
    expect(sent[1]).toEqual([1, 2, 3, 0, 0]);
  });

  it("refreshes config after a rate change and rolls back on failure", async () => {
    const { client, hook, refresh, reportError } = setup();
    act(() => hook.result.current.setOutputRate(96000));
    expect(hook.result.current.outputHz).toBe(96000);
    await waitFor(() => expect(refresh).toHaveBeenCalledWith("config"));
    expect(client.setSampleRate).toHaveBeenCalledWith({ output_hz: 96000 });

    client.setSampleRate.mockRejectedValueOnce(new Error("nope"));
    act(() => hook.result.current.setInputRate(48000));
    expect(hook.result.current.inputHz).toBe(48000);
    await waitFor(() => expect(reportError).toHaveBeenCalled());
    expect(hook.result.current.inputHz).toBe(44100);
  });
});
