import type { CdStatus, OutputInfo } from "@on-air/api-types";
import * as client from "@on-air/control-client";
import * as expoRouter from "expo-router";
import * as SecureStore from "expo-secure-store";
import { useWindowDimensions } from "react-native";
import ReactTestRenderer, { type ReactTestInstance } from "react-test-renderer";
import RootLayout from "../app/_layout";
import { FakeWebSocket } from "./helpers/fake-websocket";

jest.mock("@on-air/control-client", () => ({
  ...jest.requireActual("@on-air/control-client"),
  discoverOnAir: jest.fn(),
  fetchStatus: jest.fn(),
  verifyPin: jest.fn(),
  listInputs: jest.fn(),
  listOutputs: jest.fn(),
  getActiveInput: jest.fn(),
  getActiveOutput: jest.fn(),
  getEq: jest.fn(),
  getSampleRate: jest.fn(),
  getVolume: jest.fn(),
  getAirplayMode: jest.fn(),
  getCd: jest.fn(),
  controlCd: jest.fn(),
  activateInput: jest.fn(),
  activateOutput: jest.fn(),
  setVolume: jest.fn(),
  setEq: jest.fn(),
  setSampleRate: jest.fn(),
  pairAirplay: jest.fn(),
  pairBluetooth: jest.fn(),
  openBluetoothSettings: jest.fn(),
}));

const api = jest.mocked(client);
const store = SecureStore as jest.Mocked<typeof SecureStore> & { __reset(): void };
// `expo-router` is mapped to __mocks__/expo-router.js, which adds `__reset`.
const routerMock = expoRouter as typeof expoRouter & { __reset(): void };
const PAIRING_KEY = "on-air.desktop-pairing.v1";

const STUDIO = { host: "192.168.5.14", port: 47990, name: "studio", version: "0.1.0" };
const BASE = "http://192.168.5.14:47990";
const TOKEN = "onair-test";

const OUTPUTS: OutputInfo[] = [
  {
    id: "uuid:mock-sonos",
    name: "Mock Sonos",
    transport: "sonos",
    kind: "solo",
    member_count: 1,
    needs_pair: false,
    paired: true,
  },
  {
    id: "ap-living",
    name: "Living Room AirPlay",
    transport: "airplay",
    kind: "pair",
    member_count: 2,
    needs_pair: false,
    paired: true,
  },
  {
    id: "ap-locked",
    name: "Locked HomePod",
    transport: "airplay",
    kind: "solo",
    member_count: 1,
    needs_pair: true,
    paired: false,
  },
  {
    id: "bt-speaker",
    name: "Mock Bluetooth Speaker",
    transport: "bluetooth",
    kind: "solo",
    member_count: 1,
    needs_pair: true,
    paired: false,
  },
];

const NO_DISC: CdStatus = {
  present: false,
  playing: false,
  track: 0,
  track_count: 0,
  tracks: [],
  position_ms: 0,
  duration_ms: 0,
};

function mockDesktop() {
  api.discoverOnAir.mockResolvedValue([STUDIO]);
  api.fetchStatus.mockResolvedValue({ status: "ok", version: "0.1.0", service_enabled: true });
  api.verifyPin.mockResolvedValue(TOKEN);
  api.listInputs.mockResolvedValue(["Mock Monitor", "PipeWire Sound Server"]);
  api.listOutputs.mockResolvedValue(OUTPUTS);
  api.getActiveInput.mockResolvedValue({ name: "Mock Monitor", backend: "pipewire-monitor" });
  api.getActiveOutput.mockResolvedValue(null);
  api.getEq.mockResolvedValue([0, 0, 0, 0, 0]);
  api.getVolume.mockResolvedValue(50);
  api.getSampleRate.mockResolvedValue({
    sample_rate_hz: 44100,
    input: { sample_rate_hz: 44100, supported_hz: [44100, 48000] },
    output: { sample_rate_hz: 44100, supported_hz: [44100, 48000] },
  });
  api.getAirplayMode.mockResolvedValue("owntone");
  api.getCd.mockResolvedValue(NO_DISC);
  api.controlCd.mockResolvedValue(NO_DISC);
  api.activateInput.mockResolvedValue(undefined);
  api.activateOutput.mockResolvedValue(undefined);
  api.setVolume.mockResolvedValue(undefined);
  api.setEq.mockResolvedValue(undefined);
  api.setSampleRate.mockResolvedValue(undefined);
  api.pairAirplay.mockResolvedValue(undefined);
  api.pairBluetooth.mockResolvedValue(undefined);
  api.openBluetoothSettings.mockResolvedValue(undefined);
}

let tree: ReactTestRenderer.ReactTestRenderer | undefined;
const originalWebSocket = globalThis.WebSocket;

// Captured before any test installs fake timers, so flushing never waits on a frozen clock.
const realSetTimeout = setTimeout;

async function flush() {
  await ReactTestRenderer.act(async () => {
    for (let i = 0; i < 5; i += 1) await new Promise((resolve) => realSetTimeout(resolve, 0));
  });
}

async function renderApp() {
  await ReactTestRenderer.act(async () => {
    tree = ReactTestRenderer.create(<RootLayout />);
  });
  await flush();
  return tree as ReactTestRenderer.ReactTestRenderer;
}

/** The rendered host element: its props reflect `disabled` like the native view. */
function host(id: string): ReactTestInstance {
  const matches = (tree as ReactTestRenderer.ReactTestRenderer).root.findAll(
    (node) => typeof node.type === "string" && node.props.testID === id,
  );
  if (matches.length !== 1) throw new Error(`expected one host "${id}", found ${matches.length}`);
  return matches[0];
}

function exists(id: string): boolean {
  return (
    (tree as ReactTestRenderer.ReactTestRenderer).root.findAll(
      (node) => typeof node.type === "string" && node.props.testID === id,
    ).length > 0
  );
}

function text(): string {
  return JSON.stringify(tree?.toJSON());
}

async function press(id: string) {
  const handler = host(id).props.onPress;
  if (!handler) throw new Error(`"${id}" is disabled`);
  await ReactTestRenderer.act(async () => {
    await handler();
  });
  await flush();
}

async function pairWith(pin = "123456") {
  ReactTestRenderer.act(() => host("pin-input").props.onChangeText(pin));
  await press("pair-button");
}

beforeEach(() => {
  jest.clearAllMocks();
  routerMock.__reset();
  store.__reset();
  FakeWebSocket.reset();
  (globalThis as { WebSocket: unknown }).WebSocket = FakeWebSocket;
  jest.mocked(useWindowDimensions).mockReturnValue({
    width: 390,
    height: 844,
    scale: 3,
    fontScale: 1,
  });
  mockDesktop();
});

afterEach(() => {
  ReactTestRenderer.act(() => tree?.unmount());
  tree = undefined;
  (globalThis as { WebSocket: unknown }).WebSocket = originalWebSocket;
  jest.useRealTimers();
});

test("happy path: discover, pair, stream, adjust, and forget the desktop", async () => {
  await renderApp();

  // Progressive first-run discovery selects the Mac and reveals the code panel.
  expect(text()).toContain("Pair your Mac");
  expect(host("discovered-192.168.5.14").props.accessibilityState).toEqual({ selected: true });
  expect(text()).toContain("Ready for your code");
  expect(exists("host-input")).toBe(false);
  expect(host("pair-button").props.onPress).toBeUndefined();

  // The PIN field keeps digits only.
  ReactTestRenderer.act(() => host("pin-input").props.onChangeText("12a34b56"));
  expect(host("pin-input").props.value).toBe("123456");
  await press("pair-button");
  expect(api.verifyPin).toHaveBeenCalledWith(BASE, "123456");
  expect(store.setItemAsync).toHaveBeenCalledWith(
    PAIRING_KEY,
    JSON.stringify({ host: STUDIO.host, port: STUDIO.port, token: TOKEN }),
    expect.objectContaining({ keychainAccessible: SecureStore.WHEN_UNLOCKED_THIS_DEVICE_ONLY }),
  );

  // Mixer opens and the event socket connects.
  expect(exists("mixer-screen")).toBe(true);
  expect(text()).toContain("Mock Monitor");
  expect(text()).toContain("Choose a speaker");
  expect(host("status-pill").props.accessibilityLabel).toBe("Status: Ready");
  expect(FakeWebSocket.last?.url).toBe(`ws://192.168.5.14:47990/api/ws?token=${TOKEN}`);
  expect(host("volume-slider-native").props.onValueChange).toBeUndefined();

  // Choose a source.
  await press("source-row");
  await ReactTestRenderer.act(async () => {
    await host("source-picker").props.onValueChange("PipeWire Sound Server");
  });
  await flush();
  expect(api.activateInput).toHaveBeenCalledWith(BASE, "PipeWire Sound Server", TOKEN);
  expect(exists("source-sheet")).toBe(false);

  // Choose a speaker; the desktop's event marks it live without a full refetch.
  await press("output-row");
  expect(text()).toContain("Living Room AirPlay");
  expect(text()).toContain("Stereo pair · airplay");
  const listsBefore = api.listInputs.mock.calls.length;
  api.getActiveOutput.mockResolvedValue({
    transport: "sonos",
    device_id: "uuid:mock-sonos",
    device_name: "Mock Sonos",
  });
  await press("output-sonos-uuid:mock-sonos");
  expect(api.activateOutput).toHaveBeenCalledWith(BASE, "sonos", "uuid:mock-sonos", TOKEN);
  expect(exists("output-sheet")).toBe(false);
  ReactTestRenderer.act(() => FakeWebSocket.last?.open());
  ReactTestRenderer.act(() =>
    FakeWebSocket.last?.emit({
      type: "OutputStateChanged",
      transport: "sonos",
      device_name: "Mock Sonos",
      active: true,
    }),
  );
  expect(host("status-pill").props.accessibilityLabel).toBe("Audio is live");
  expect(text()).toContain("Audio is live");
  expect(api.listInputs.mock.calls.length).toBe(listsBefore);

  // Volume commits after the drag pauses.
  jest.useFakeTimers();
  ReactTestRenderer.act(() => host("volume-slider-native").props.onValueChange(73));
  expect(api.setVolume).not.toHaveBeenCalled();
  await ReactTestRenderer.act(async () => {
    jest.advanceTimersByTime(180);
  });
  expect(api.setVolume).toHaveBeenCalledWith(BASE, 73, TOKEN);

  // Equalizer: two quick band edits keep both values and serialise.
  await press("sound-settings-button");
  expect(text()).toContain("Audio format");
  await press("equalizer-link");
  expect(text()).toContain("Bass · 60 Hz");
  ReactTestRenderer.act(() => {
    host("eq-band-0-native").props.onValueChange(0.5);
    host("eq-band-1-native").props.onValueChange(1);
  });
  await ReactTestRenderer.act(async () => {
    jest.advanceTimersByTime(180);
  });
  jest.useRealTimers();
  await flush();
  expect(api.setEq).toHaveBeenLastCalledWith(BASE, [0.5, 1, 0, 0, 0], TOKEN);

  // Audio format.
  ReactTestRenderer.act(() => expoRouter.router.back());
  await press("audio-format-link");
  await ReactTestRenderer.act(async () => {
    await host("output-sample-rate-picker").props.onValueChange(48000);
  });
  expect(api.setSampleRate).toHaveBeenCalledWith(BASE, { output_hz: 48000 }, TOKEN);

  // Back to the mixer, then Connection shows the paired host, and Forget returns to pairing.
  await press("native-back");
  await press("native-back");
  expect(exists("mixer-screen")).toBe(true);
  await press("more-button");
  expect(host("paired-host").props.children).toBe(STUDIO.host);
  expect(text()).toContain("Connected to desktop");
  await press("disconnect-menu-button");
  expect(text()).toContain("Pair your Mac");
  expect(store.deleteItemAsync).toHaveBeenCalledWith(PAIRING_KEY);
  expect(FakeWebSocket.last?.closed).toBe(true);
});

test("restores a saved pairing, including a non-default port, without discovery", async () => {
  await store.setItemAsync(
    PAIRING_KEY,
    JSON.stringify({ host: "192.168.5.14", port: 48000, token: "saved-token" }),
  );
  jest.clearAllMocks();
  await renderApp();
  expect(exists("mixer-screen")).toBe(true);
  expect(api.verifyPin).not.toHaveBeenCalled();
  expect(api.discoverOnAir).not.toHaveBeenCalled();
  expect(api.listInputs).toHaveBeenCalledWith("http://192.168.5.14:48000", "saved-token");
  await press("more-button");
  expect(host("paired-host").props.children).toBe("192.168.5.14:48000");
});

test("pairs with a discovered desktop on its advertised port", async () => {
  api.discoverOnAir.mockResolvedValue([{ ...STUDIO, port: 48123 }]);
  await renderApp();
  await pairWith();
  expect(api.verifyPin).toHaveBeenCalledWith("http://192.168.5.14:48123", "123456");
  expect(exists("mixer-screen")).toBe(true);
});

test("a keychain write failure still opens the mixer with a warning", async () => {
  jest.spyOn(console, "warn").mockImplementation(() => {});
  store.setItemAsync.mockRejectedValueOnce(new Error("keychain unavailable"));
  await renderApp();
  await pairWith();
  expect(exists("mixer-screen")).toBe(true);
  expect(text()).toContain("could not save the pairing");
  expect(api.listInputs).toHaveBeenCalledWith(BASE, TOKEN);
});

test("pairing refuses a paused desktop before spending the code", async () => {
  api.fetchStatus.mockResolvedValue({ status: "ok", version: "0.1.0", service_enabled: false });
  await renderApp();
  await pairWith();
  expect(api.verifyPin).not.toHaveBeenCalled();
  expect(host("pairing-error").props.children).toContain("service is paused");
});

test("a wrong code shows the envelope's copy and keeps the pairing screen", async () => {
  api.verifyPin.mockRejectedValueOnce(
    new client.HttpError("/api/pairing/verify", 401, '{"error":"bad pin","code":"invalid_pin"}'),
  );
  await renderApp();
  await pairWith();
  expect(exists("pairing-screen")).toBe(true);
  expect(host("pairing-error").props.children).toContain("That code was not accepted");
  expect(store.setItemAsync).not.toHaveBeenCalled();
});

test("a late scan lists the desktop but keeps a manually typed address", async () => {
  let finish!: (hits: (typeof STUDIO)[]) => void;
  api.discoverOnAir.mockReturnValueOnce(new Promise((resolve) => (finish = resolve)));
  await renderApp();
  expect(text()).toContain("Looking for your Mac");
  expect(exists("pin-input")).toBe(false);
  await press("manual-setup-button");
  ReactTestRenderer.act(() => host("host-input").props.onChangeText("192.168.5.77"));
  finish([STUDIO]);
  await flush();
  expect(host("host-input").props.value).toBe("192.168.5.77");
  expect(exists("discovered-192.168.5.14")).toBe(true);
  await pairWith();
  expect(api.verifyPin).toHaveBeenCalledWith("http://192.168.5.77:47990", "123456");
});

test("an expired pairing returns to pairing with the desktop still selected", async () => {
  await renderApp();
  await pairWith();
  api.fetchStatus.mockRejectedValueOnce(new Error("Timeout"));
  api.listInputs.mockRejectedValueOnce(
    new client.HttpError("/api/inputs", 401, '{"error":"","code":"not_paired"}'),
  );
  await press("more-button");
  await press("refresh-button");
  expect(text()).toContain("Pair your Mac");
  expect(text()).toContain("Pairing expired");
  expect(exists("pin-input")).toBe(true);
  expect(store.deleteItemAsync).toHaveBeenCalledWith(PAIRING_KEY);
});

test("a paused desktop keeps its pairing and disables sound settings", async () => {
  await renderApp();
  await pairWith();
  api.fetchStatus.mockResolvedValue({ status: "ok", version: "0.1.0", service_enabled: false });
  for (const read of [api.listInputs, api.listOutputs, api.getEq, api.getVolume]) {
    read.mockRejectedValue(
      new client.HttpError("/api/x", 503, '{"error":"","code":"service_paused"}'),
    );
  }
  await press("more-button");
  await press("refresh-button");
  expect(text()).toContain("Connected to desktop");
  expect(text()).toContain("desktop service is paused");
  await press("native-back");
  expect(host("status-pill").props.accessibilityLabel).toBe("Desktop service is paused");
  expect(host("sound-settings-button").props.onPress).toBeUndefined();
  expect(store.deleteItemAsync).not.toHaveBeenCalled();
});

test("unpaired AirPlay asks for the speaker code, then pairs and connects", async () => {
  await renderApp();
  await pairWith();
  await press("output-row");
  await press("output-airplay-ap-locked");
  expect(exists("output-sheet")).toBe(false);
  expect(exists("pair-sheet")).toBe(true);
  expect(api.activateOutput).not.toHaveBeenCalled();
  ReactTestRenderer.act(() => host("device-pin-input").props.onChangeText("11x11"));
  await press("device-pair-submit");
  expect(api.pairAirplay).toHaveBeenCalledWith(BASE, "ap-locked", "1111", TOKEN);
  expect(api.activateOutput).toHaveBeenCalledWith(BASE, "airplay", "ap-locked", TOKEN);
  expect(exists("pair-sheet")).toBe(false);
});

test("compact disc transport applies the returned state", async () => {
  api.getCd.mockResolvedValue({
    ...NO_DISC,
    present: true,
    playing: true,
    track: 1,
    track_count: 2,
    title: "So What",
    album: "Kind of Blue",
  });
  api.controlCd.mockResolvedValue({
    ...NO_DISC,
    present: true,
    playing: true,
    track: 2,
    track_count: 2,
    title: "Freddie Freeloader",
  });
  await renderApp();
  await pairWith();
  expect(text()).toContain("So What");
  await press("cd-next");
  expect(api.controlCd).toHaveBeenCalledWith(BASE, "next", TOKEN);
  expect(text()).toContain("Freddie Freeloader");
});

test("layouts: two panes when unfolded, compact rows in phone landscape", async () => {
  jest.mocked(useWindowDimensions).mockReturnValue({
    width: 852,
    height: 883,
    scale: 2.4375,
    fontScale: 1,
  });
  await renderApp();
  expect(exists("pairing-wide-layout")).toBe(true);
  await pairWith();
  expect(exists("mixer-wide-layout")).toBe(true);

  jest.mocked(useWindowDimensions).mockReturnValue({
    width: 844,
    height: 390,
    scale: 3,
    fontScale: 1,
  });
  ReactTestRenderer.act(() => tree?.update(<RootLayout />));
  expect(exists("mixer-landscape-layout")).toBe(true);
  for (const id of ["source-row", "output-row", "volume-slider-native", "disconnect-button"]) {
    expect(exists(id)).toBe(true);
  }
});

test("accessibility: nothing but the host string is selectable; decorative icons are hidden", async () => {
  await renderApp();
  await pairWith();
  const root = (tree as ReactTestRenderer.ReactTestRenderer).root;
  expect(root.findAll((node) => node.props.selectable === true)).toHaveLength(0);
  const labelledIcons = root.findAll(
    (node) => (node.type as unknown) === "ExpoIcon" && node.props.accessibilityLabel !== undefined,
  );
  expect(labelledIcons).toHaveLength(0);
  const pill = host("status-pill");
  expect(pill.props.accessible).toBe(true);
  expect(pill.props.accessibilityRole).toBe("text");
  // RN content must not be nested directly in a SwiftUI Host.
  let parent = root.findByProps({ testID: "mixer-screen" }).parent;
  while (parent) {
    expect(parent.type).not.toBe("ExpoHost");
    parent = parent.parent;
  }
});
