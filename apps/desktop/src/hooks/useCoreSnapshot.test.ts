import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { FakeSocket, fakeClient, fakeCore, homepod, sonos, tick } from "../test/fakes";
import { useCoreSnapshot } from "./useCoreSnapshot";

const DEBOUNCE = 20;

function setup(core = fakeCore()) {
  const client = fakeClient(core);
  const hook = renderHook(() => useCoreSnapshot({ client, debounceMs: DEBOUNCE, pollMs: 60_000 }));
  return { core, client, hook };
}

async function loaded(hook: ReturnType<typeof setup>["hook"]) {
  await waitFor(() => expect(hook.result.current.snapshot.outputs).not.toBeNull());
}

beforeEach(() => {
  FakeSocket.reset();
});

describe("useCoreSnapshot", () => {
  it("starts with a null snapshot and a connecting lamp, then fills in", async () => {
    const { hook } = setup();
    expect(hook.result.current.snapshot.outputs).toBeNull();
    expect(hook.result.current.snapshot.inputs).toBeNull();
    expect(hook.result.current.connection).toBe("connecting");
    await loaded(hook);
    expect(hook.result.current.connection).toBe("ok");
    expect(hook.result.current.snapshot.outputs).toEqual([sonos, homepod]);
    expect(hook.result.current.snapshot.pin).toBe("123456");
    expect(hook.result.current.snapshot.status?.lan_addresses).toEqual(["10.0.0.2"]);
  });

  it("applies WebSocket payloads directly without refetching", async () => {
    const { hook, client } = setup();
    await loaded(hook);
    const socket = FakeSocket.latest();
    act(() => socket.open());
    const cdCalls = client.getCd.mock.calls.length;
    const activeCalls = client.getActiveOutput.mock.calls.length;

    act(() =>
      socket.send({
        type: "CdStateChanged",
        present: true,
        playing: true,
        track: 2,
        track_count: 9,
        album: "Kind of Blue",
        position_ms: 1000,
        duration_ms: 90_000,
      }),
    );
    expect(hook.result.current.snapshot.cd?.track).toBe(2);
    expect(hook.result.current.snapshot.cd?.album).toBe("Kind of Blue");

    act(() =>
      socket.send({
        type: "OutputStateChanged",
        transport: "sonos",
        device_name: "Kitchen",
        active: true,
      }),
    );
    expect(hook.result.current.snapshot.activeOutput).toEqual({
      transport: "sonos",
      device_id: "uuid:mock-sonos",
      device_name: "Kitchen",
    });

    act(() =>
      socket.send({
        type: "OutputStateChanged",
        transport: "sonos",
        device_name: "Kitchen",
        active: false,
      }),
    );
    expect(hook.result.current.snapshot.activeOutput).toBeNull();

    act(() => socket.send({ type: "DeviceLeft", transport: "airplay", id: "hp-1" }));
    expect(hook.result.current.snapshot.outputs?.map((o) => o.id)).toEqual(["uuid:mock-sonos"]);

    act(() => socket.send({ type: "ServiceStateChanged", enabled: false }));
    expect(hook.result.current.connection).toBe("paused");
    expect(hook.result.current.snapshot.status?.service_enabled).toBe(false);

    await tick(DEBOUNCE * 2);
    expect(client.getCd.mock.calls.length).toBe(cdCalls);
    expect(client.getActiveOutput.mock.calls.length).toBe(activeCalls);
  });

  it("patches a joined device in place and then fills its details from one refresh", async () => {
    const { hook, client, core } = setup();
    await loaded(hook);
    const socket = FakeSocket.latest();
    act(() => socket.open());
    const before = client.listOutputs.mock.calls.length;
    core.outputs = [...core.outputs, { ...sonos, id: "uuid:new", name: "Patio" }];
    act(() =>
      socket.send({ type: "DeviceJoined", transport: "sonos", id: "uuid:new", name: "Patio" }),
    );
    expect(hook.result.current.snapshot.outputs?.some((o) => o.id === "uuid:new")).toBe(true);
    await waitFor(() => expect(client.listOutputs.mock.calls.length).toBe(before + 1));
    await tick(DEBOUNCE * 2);
    expect(client.listOutputs.mock.calls.length).toBe(before + 1);
  });

  it("coalesces a burst of refresh calls into a leading and one trailing fetch", async () => {
    const { hook, client } = setup();
    await loaded(hook);
    await tick(DEBOUNCE * 2);
    const outputsBefore = client.listOutputs.mock.calls.length;
    const volumeBefore = client.getVolume.mock.calls.length;
    let burst: Promise<void> = Promise.resolve();
    act(() => {
      for (let i = 0; i < 5; i += 1) burst = hook.result.current.refresh("devices");
    });
    // The leading run starts at once (status first, then the group), still inside the window.
    await tick(0);
    expect(client.listOutputs.mock.calls.length).toBe(outputsBefore + 1);
    await act(async () => {
      await burst;
    });
    await tick(DEBOUNCE * 3);
    expect(client.listOutputs.mock.calls.length).toBe(outputsBefore + 2);
    // A devices refresh never touches the config group.
    expect(client.getVolume.mock.calls.length).toBe(volumeBefore);
  });

  it("skips device and config fetches while the service is paused", async () => {
    const core = fakeCore();
    core.status = { ...core.status, service_enabled: false };
    const { hook, client } = setup(core);
    await waitFor(() => expect(hook.result.current.connection).toBe("paused"));
    await tick(DEBOUNCE * 2);
    expect(client.listOutputs).not.toHaveBeenCalled();
    expect(client.getVolume).not.toHaveBeenCalled();
    expect(client.fetchStatus).toHaveBeenCalled();
    expect(hook.result.current.snapshot.status?.service_enabled).toBe(false);

    core.status = { ...core.status, service_enabled: true };
    await act(async () => {
      await hook.result.current.refresh("all");
    });
    await waitFor(() => expect(hook.result.current.connection).toBe("ok"));
    expect(client.listOutputs).toHaveBeenCalled();
  });

  it("keeps an action error across refreshes until it is dismissed", async () => {
    const { hook } = setup();
    await loaded(hook);
    act(() => hook.result.current.reportError(new Error("Speaker said no")));
    expect(hook.result.current.actionError?.message).toBe("Speaker said no");
    await act(async () => {
      await hook.result.current.refresh("all");
    });
    await tick(DEBOUNCE * 2);
    expect(hook.result.current.actionError?.message).toBe("Speaker said no");
    act(() => hook.result.current.clearError());
    expect(hook.result.current.actionError).toBeNull();
  });

  it("marks the core unreachable when status fails and keeps the last snapshot", async () => {
    const { hook, client } = setup();
    await loaded(hook);
    client.fetchStatus.mockRejectedValueOnce(new TypeError("Failed to fetch"));
    await act(async () => {
      await hook.result.current.refresh("all");
    });
    expect(hook.result.current.connection).toBe("unreachable");
    expect(hook.result.current.snapshot.outputs).toEqual([sonos, homepod]);
    expect(hook.result.current.actionError).toBeNull();
  });

  it("reports live updates as reconnecting after the socket drops", async () => {
    const { hook } = setup();
    await loaded(hook);
    const socket = FakeSocket.latest();
    act(() => socket.open());
    expect(hook.result.current.liveUpdates).toBe("open");
    act(() => socket.dropFromServer());
    expect(hook.result.current.liveUpdates).toBe("reconnecting");
    await waitFor(() => expect(FakeSocket.instances.length).toBeGreaterThan(1));
    act(() => FakeSocket.latest().open());
    expect(hook.result.current.liveUpdates).toBe("open");
    hook.unmount();
    expect(FakeSocket.latest().closed).toBe(true);
  });
});
