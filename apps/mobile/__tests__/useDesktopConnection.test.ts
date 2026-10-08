import * as client from "@on-air/control-client";
import { act, renderHook } from "@testing-library/react-native";
import {
  OFFLINE_ERROR,
  PARTIAL_REFRESH_ERROR,
  useDesktopConnection,
} from "@/hooks/useDesktopConnection";
import { FakeWebSocket } from "./helpers/fake-websocket";

jest.mock("@on-air/control-client", () => ({
  ...jest.requireActual("@on-air/control-client"),
  fetchStatus: jest.fn(),
  listInputs: jest.fn(),
  listOutputs: jest.fn(),
  getActiveInput: jest.fn(),
  getActiveOutput: jest.fn(),
  getVolume: jest.fn(),
  getEq: jest.fn(),
  getSampleRate: jest.fn(),
  getAirplayMode: jest.fn(),
  getCd: jest.fn(),
}));

const reads = {
  fetchStatus: jest.mocked(client.fetchStatus),
  listInputs: jest.mocked(client.listInputs),
  listOutputs: jest.mocked(client.listOutputs),
  getActiveInput: jest.mocked(client.getActiveInput),
  getActiveOutput: jest.mocked(client.getActiveOutput),
  getVolume: jest.mocked(client.getVolume),
  getEq: jest.mocked(client.getEq),
  getSampleRate: jest.mocked(client.getSampleRate),
  getAirplayMode: jest.mocked(client.getAirplayMode),
  getCd: jest.mocked(client.getCd),
};
const allReads = Object.values(reads);

const BASE = "http://192.168.5.14:47990";
const SONOS = {
  id: "uuid:sonos",
  name: "Mock Sonos",
  transport: "sonos" as const,
  kind: "solo" as const,
  member_count: 1,
  needs_pair: false,
  paired: true,
};

function happyReads() {
  reads.fetchStatus.mockResolvedValue({ status: "ok", version: "0.1.0", service_enabled: true });
  reads.listInputs.mockResolvedValue(["Mock Monitor"]);
  reads.listOutputs.mockResolvedValue([SONOS]);
  reads.getActiveInput.mockResolvedValue({ name: "Mock Monitor", backend: "cpal-default" });
  reads.getActiveOutput.mockResolvedValue(null);
  reads.getVolume.mockResolvedValue(42);
  reads.getEq.mockResolvedValue([0, 0, 0, 0, 0]);
  reads.getSampleRate.mockResolvedValue({
    sample_rate_hz: 44100,
    input: { sample_rate_hz: 44100, supported_hz: [44100, 48000] },
    output: { sample_rate_hz: 48000, supported_hz: [48000] },
  });
  reads.getAirplayMode.mockResolvedValue("owntone");
  reads.getCd.mockResolvedValue({
    present: false,
    playing: false,
    track: 0,
    track_count: 0,
    tracks: [],
    position_ms: 0,
    duration_ms: 0,
  });
}

const settle = () =>
  act(async () => {
    for (let i = 0; i < 12; i += 1) await Promise.resolve();
  });

const tick = (ms: number) =>
  act(async () => {
    jest.advanceTimersByTime(ms);
    for (let i = 0; i < 12; i += 1) await Promise.resolve();
  });

function callCounts() {
  return Object.fromEntries(
    Object.entries(reads).map(([name, fn]) => [name, fn.mock.calls.length]),
  );
}

type Props = Partial<Parameters<typeof useDesktopConnection>[0]>;

function connect(overrides: Props = {}) {
  const onUnauthorized = jest.fn();
  const initial: Parameters<typeof useDesktopConnection>[0] = {
    base: BASE,
    token: "tok",
    active: true,
    onUnauthorized,
    WebSocket: FakeWebSocket,
    pollMs: 1_000,
    coalesceMs: 100,
    stallMs: 0,
    ...overrides,
  };
  const hook = renderHook(
    (props: Parameters<typeof useDesktopConnection>[0]) => useDesktopConnection(props),
    {
      initialProps: initial,
    },
  );
  return { ...hook, onUnauthorized, initial };
}

beforeEach(() => {
  jest.useFakeTimers();
  jest.clearAllMocks();
  FakeWebSocket.reset();
  happyReads();
});

afterEach(() => {
  jest.useRealTimers();
});

test("connects with one immediate refresh and opens the event socket", async () => {
  const { result } = connect();
  expect(result.current.phase).toBe("connecting");
  expect(FakeWebSocket.instances).toHaveLength(1);
  expect(FakeWebSocket.last?.url).toContain("/api/ws?token=tok");
  await settle();
  expect(result.current.phase).toBe("connected");
  expect(result.current.wsState).toBe("connecting");
  expect(result.current.inputs).toEqual(["Mock Monitor"]);
  expect(result.current.volume).toBe(42);
  expect(result.current.outputSampleRate).toBe(48000);
  expect(result.current.outputRates).toEqual([48000]);
  expect(result.current.error).toBeNull();
  for (const read of allReads) expect(read).toHaveBeenCalledTimes(1);
  act(() => FakeWebSocket.last?.open());
  expect(result.current.wsState).toBe("open");
  await tick(500);
  for (const read of allReads) expect(read).toHaveBeenCalledTimes(1);
});

test("applies socket payloads directly and refetches only the named slice", async () => {
  const { result } = connect();
  await settle();
  act(() => FakeWebSocket.last?.open());
  const before = callCounts();

  act(() =>
    FakeWebSocket.last?.emit({
      type: "OutputStateChanged",
      transport: "sonos",
      device_name: "Mock Sonos",
      active: true,
    }),
  );
  expect(result.current.activeOutput).toEqual({
    transport: "sonos",
    device_id: "uuid:sonos",
    device_name: "Mock Sonos",
  });
  act(() =>
    FakeWebSocket.last?.emit({
      type: "CdStateChanged",
      present: false,
      playing: false,
      track: 3,
      track_count: 0,
      position_ms: 10,
      duration_ms: 0,
    }),
  );
  expect(result.current.cd.track).toBe(3);
  act(() =>
    FakeWebSocket.last?.emit({ type: "DeviceJoined", transport: "sonos", id: "x", name: "Y" }),
  );
  act(() => FakeWebSocket.last?.emit({ type: "LevelMeter", rms: 0.1, peak: 0.2 }));
  expect(callCounts()).toEqual(before);

  await tick(150);
  expect(reads.getVolume).toHaveBeenCalledTimes(before.getVolume + 1);
  expect(reads.listOutputs).toHaveBeenCalledTimes(before.listOutputs + 1);
  expect(reads.getActiveOutput).toHaveBeenCalledTimes(before.getActiveOutput);
  expect(reads.getCd).toHaveBeenCalledTimes(before.getCd);
  expect(reads.listInputs).toHaveBeenCalledTimes(before.listInputs);

  act(() =>
    FakeWebSocket.last?.emit({
      type: "OutputStateChanged",
      transport: "sonos",
      device_name: "Mock Sonos",
      active: false,
    }),
  );
  expect(result.current.activeOutput).toBeNull();
});

test("a disc change refetches the CD slice for its track list", async () => {
  const { result } = connect();
  await settle();
  act(() => FakeWebSocket.last?.open());
  const before = callCounts();
  reads.getCd.mockResolvedValue({
    present: true,
    playing: true,
    track: 1,
    track_count: 2,
    tracks: [
      { number: 1, duration_ms: 1 },
      { number: 2, duration_ms: 2 },
    ],
    position_ms: 0,
    duration_ms: 1,
  });
  act(() =>
    FakeWebSocket.last?.emit({
      type: "CdStateChanged",
      present: true,
      playing: true,
      track: 1,
      track_count: 2,
      position_ms: 0,
      duration_ms: 1,
    }),
  );
  expect(result.current.cd.present).toBe(true);
  await tick(150);
  expect(reads.getCd).toHaveBeenCalledTimes(before.getCd + 1);
  expect(result.current.cd.tracks).toHaveLength(2);
  expect(callCounts().listInputs).toBe(before.listInputs);
});

test("coalesces a burst of refresh requests into one batch of the union", async () => {
  const { result } = connect();
  await settle();
  const before = callCounts();
  act(() => {
    result.current.scheduleRefresh("event", ["outputs"]);
    result.current.scheduleRefresh("event", ["outputs"]);
    result.current.scheduleRefresh("control", ["activeInput"]);
  });
  await tick(50);
  expect(callCounts()).toEqual(before);
  await tick(100);
  expect(reads.listOutputs).toHaveBeenCalledTimes(before.listOutputs + 1);
  expect(reads.getActiveInput).toHaveBeenCalledTimes(before.getActiveInput + 1);
  expect(reads.fetchStatus).toHaveBeenCalledTimes(before.fetchStatus);
});

test("requests arriving during a refresh run once more after it, not in parallel", async () => {
  let release!: (value: string[]) => void;
  reads.listInputs.mockImplementationOnce(() => new Promise((resolve) => (release = resolve)));
  const { result } = connect();
  act(() => {
    result.current.scheduleRefresh("event", ["inputs"]);
    result.current.scheduleRefresh("event", ["inputs"]);
  });
  await tick(150);
  expect(reads.listInputs).toHaveBeenCalledTimes(1);
  release(["Mock Monitor"]);
  await settle();
  expect(reads.listInputs).toHaveBeenCalledTimes(2);
  expect(reads.fetchStatus).toHaveBeenCalledTimes(1);
});

test("a paused service is a reachable desktop with no error", async () => {
  reads.fetchStatus.mockResolvedValue({ status: "ok", version: "0.1.0", service_enabled: false });
  for (const read of allReads) {
    if (read !== reads.fetchStatus) {
      read.mockRejectedValue(
        new client.HttpError("/api/x", 503, '{"error":"","code":"service_paused"}'),
      );
    }
  }
  const { result } = connect();
  await settle();
  expect(result.current.phase).toBe("paused");
  expect(result.current.error).toBeNull();
  expect(result.current.status?.service_enabled).toBe(false);
});

test("ServiceStateChanged toggles paused and resumes with a refresh", async () => {
  const { result } = connect();
  await settle();
  act(() => FakeWebSocket.last?.open());
  act(() => FakeWebSocket.last?.emit({ type: "ServiceStateChanged", enabled: false }));
  expect(result.current.phase).toBe("paused");
  expect(result.current.status?.service_enabled).toBe(false);
  const before = callCounts();
  act(() => FakeWebSocket.last?.emit({ type: "ServiceStateChanged", enabled: true }));
  expect(result.current.phase).toBe("connected");
  await tick(150);
  expect(reads.fetchStatus).toHaveBeenCalledTimes(before.fetchStatus + 1);
});

test("a 401 on any read wins over other failures and reports unauthorized once", async () => {
  reads.fetchStatus.mockRejectedValueOnce(new Error("timeout"));
  reads.listInputs.mockRejectedValueOnce(
    new client.HttpError("/api/inputs", 401, '{"error":"","code":"not_paired"}'),
  );
  const { result, onUnauthorized } = connect();
  await settle();
  expect(result.current.phase).toBe("unauthorized");
  expect(onUnauthorized).toHaveBeenCalledTimes(1);
  expect(onUnauthorized).toHaveBeenCalledWith(expect.stringContaining("Pairing expired"));
  expect(result.current.inputs).toEqual([]);
});

test("an outage reports reconnecting without losing the snapshot, then recovers", async () => {
  const { result } = connect();
  await settle();
  for (const read of allReads) read.mockRejectedValueOnce(new Error("Network request failed"));
  let finished = false;
  let refresh!: Promise<void>;
  act(() => {
    refresh = result.current.refreshNow().then(() => {
      finished = true;
    });
  });
  expect(result.current.refreshing).toBe(true);
  await settle();
  await refresh;
  expect(finished).toBe(true);
  expect(result.current.refreshing).toBe(false);
  expect(result.current.phase).toBe("reconnecting");
  expect(result.current.error).toBe(OFFLINE_ERROR);
  expect(result.current.inputs).toEqual(["Mock Monitor"]);
  await act(() => result.current.refreshNow());
  expect(result.current.phase).toBe("connected");
  expect(result.current.error).toBeNull();
});

test("a failed required slice is partial; a missing optional endpoint is fine", async () => {
  reads.getCd.mockRejectedValue(new client.HttpError("/api/cd", 404));
  const { result } = connect();
  await settle();
  expect(result.current.phase).toBe("connected");
  expect(result.current.error).toBeNull();
  reads.getEq.mockRejectedValueOnce(new Error("slow"));
  await act(() => result.current.refreshNow());
  expect(result.current.phase).toBe("connected");
  expect(result.current.error).toBe(PARTIAL_REFRESH_ERROR);
});

test("polls on the interval only while the app is active", async () => {
  const { result, rerender, initial } = connect();
  await settle();
  await tick(1_000);
  expect(reads.fetchStatus).toHaveBeenCalledTimes(1);
  await tick(100);
  expect(reads.fetchStatus).toHaveBeenCalledTimes(2);
  rerender({ ...initial, active: false });
  expect(FakeWebSocket.last?.closed).toBe(true);
  expect(result.current.wsState).toBe("closed");
  await tick(3_000);
  expect(reads.fetchStatus).toHaveBeenCalledTimes(2);
  rerender({ ...initial, active: true });
  await settle();
  expect(reads.fetchStatus).toHaveBeenCalledTimes(3);
  expect(FakeWebSocket.instances).toHaveLength(2);
  expect(result.current.phase).toBe("connected");
});

test("a socket loss schedules a refresh and a reopen after loss refreshes again", async () => {
  const { result } = connect({ pollMs: 600_000 });
  await settle();
  act(() => FakeWebSocket.last?.open());
  const before = callCounts();
  act(() => FakeWebSocket.last?.dropFromServer());
  expect(result.current.wsState).toBe("reconnecting");
  await tick(150);
  expect(reads.fetchStatus).toHaveBeenCalledTimes(before.fetchStatus + 1);
  await tick(2_000);
  expect(FakeWebSocket.instances.length).toBeGreaterThan(1);
  act(() => FakeWebSocket.last?.open());
  await tick(150);
  expect(reads.fetchStatus).toHaveBeenCalledTimes(before.fetchStatus + 2);
});

test("results from a previous pairing are dropped after the token changes", async () => {
  let reject!: (reason: unknown) => void;
  reads.listInputs.mockImplementationOnce(() => new Promise((_, rej) => (reject = rej)));
  const { result, rerender, initial, onUnauthorized } = connect();
  rerender({ ...initial, token: null });
  expect(result.current.phase).toBe("idle");
  rerender({ ...initial, token: "tok2" });
  await settle();
  reject(new client.HttpError("/api/inputs", 401));
  await settle();
  expect(onUnauthorized).not.toHaveBeenCalled();
  expect(result.current.phase).toBe("connected");
  expect(FakeWebSocket.last?.url).toContain("token=tok2");
});

test("unmounting cancels the poll, the coalesced refresh and the socket", async () => {
  const { result, unmount } = connect();
  await settle();
  act(() => result.current.scheduleRefresh("poll"));
  const socket = FakeWebSocket.last;
  unmount();
  expect(socket?.closed).toBe(true);
  const before = callCounts();
  // React's act can leave a fake microtask queued; it must run without side effects.
  jest.advanceTimersByTime(60_000);
  expect(callCounts()).toEqual(before);
  expect(jest.getTimerCount()).toBe(0);
  expect(FakeWebSocket.instances).toHaveLength(1);
});

test("patch updates the snapshot locally", async () => {
  const { result } = connect();
  await settle();
  act(() => result.current.patch({ volume: 77, gains: [1, 0, 0, 0, 0] }));
  expect(result.current.volume).toBe(77);
  expect(result.current.gains).toEqual([1, 0, 0, 0, 0]);
});

test("an unreachable optional-only refresh keeps the phase; an HTTP error still counts as reached", async () => {
  const { result } = connect();
  await settle();
  reads.getCd.mockRejectedValueOnce(new Error("Network request failed"));
  await act(() => result.current.refreshNow(["cd"]));
  expect(result.current.phase).toBe("connected");
  expect(result.current.error).toBeNull();
  for (const read of allReads) read.mockRejectedValueOnce(new client.HttpError("/api/x", 500));
  await act(() => result.current.refreshNow());
  expect(result.current.phase).toBe("connected");
  expect(result.current.error).toBe(PARTIAL_REFRESH_ERROR);
});
