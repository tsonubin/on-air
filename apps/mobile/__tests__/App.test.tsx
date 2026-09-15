import * as SecureStore from "expo-secure-store";
import { useWindowDimensions } from "react-native";
import ReactTestRenderer from "react-test-renderer";
import App from "../App";
import * as client from "../src/controlClient";

jest.mock("../src/controlClient", () => {
  const actual = jest.requireActual("../src/controlClient");
  return {
    ...actual,
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
  };
});

const mocked = client as unknown as {
  discoverOnAir: jest.Mock;
  fetchStatus: jest.Mock;
  verifyPin: jest.Mock;
  listInputs: jest.Mock;
  listOutputs: jest.Mock;
  getActiveInput: jest.Mock;
  getActiveOutput: jest.Mock;
  getEq: jest.Mock;
  getSampleRate: jest.Mock;
  getVolume: jest.Mock;
  getAirplayMode: jest.Mock;
  getCd: jest.Mock;
  controlCd: jest.Mock;
  activateInput: jest.Mock;
  activateOutput: jest.Mock;
  setVolume: jest.Mock;
  setEq: jest.Mock;
  setSampleRate: jest.Mock;
  pairAirplay: jest.Mock;
  pairBluetooth: jest.Mock;
};

const STUDIO = { host: "192.168.5.14", port: 47990, name: "studio", version: "0.1.0" };

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

function mockMixer() {
  mocked.discoverOnAir.mockResolvedValue([STUDIO]);
  mocked.fetchStatus.mockResolvedValue({ status: "ok", version: "0.1.0", service_enabled: true });
  mocked.verifyPin.mockResolvedValue("onair-test");
  mocked.listInputs.mockResolvedValue(["Mock Monitor", "PipeWire Sound Server"]);
  mocked.listOutputs.mockResolvedValue([
    {
      id: "uuid:mock-sonos",
      name: "Mock Sonos",
      transport: "sonos",
      kind: "solo",
      member_count: 1,
      paired: true,
    },
    {
      id: "ap-living",
      name: "Living Room AirPlay",
      transport: "airplay",
      kind: "pair",
      member_count: 2,
      paired: true,
    },
    {
      id: "ap-locked",
      name: "Locked HomePod",
      transport: "airplay",
      needs_pair: true,
      paired: false,
    },
    {
      id: "bt-speaker",
      name: "Mock Bluetooth Speaker",
      transport: "bluetooth",
      needs_pair: true,
      paired: false,
    },
  ]);
  mocked.getActiveInput.mockResolvedValue({ name: "Mock Monitor", backend: "mock" });
  mocked.getActiveOutput.mockResolvedValue(null);
  mocked.getEq.mockResolvedValue([0, 0, 0, 0, 0]);
  mocked.getVolume.mockResolvedValue(50);
  mocked.getSampleRate.mockResolvedValue({
    sample_rate_hz: 44100,
    input: { sample_rate_hz: 44100, supported_hz: [44100, 48000] },
    output: { sample_rate_hz: 44100, supported_hz: [44100, 48000] },
  });
  mocked.getAirplayMode.mockResolvedValue("owntone");
  mocked.getCd.mockResolvedValue({
    present: false,
    playing: false,
    track: 0,
    track_count: 0,
    position_ms: 0,
    duration_ms: 0,
  });
  mocked.controlCd.mockResolvedValue({
    present: false,
    playing: false,
    track: 0,
    track_count: 0,
    position_ms: 0,
    duration_ms: 0,
  });
  mocked.activateInput.mockResolvedValue(undefined);
  mocked.activateOutput.mockResolvedValue(undefined);
  mocked.setVolume.mockResolvedValue(undefined);
  mocked.setEq.mockResolvedValue(undefined);
  mocked.setSampleRate.mockResolvedValue(undefined);
  mocked.pairAirplay.mockResolvedValue(undefined);
  mocked.pairBluetooth.mockResolvedValue(undefined);
}

async function flush() {
  await ReactTestRenderer.act(async () => {
    await new Promise((resolve) => setImmediate(resolve));
  });
}

let tree: ReactTestRenderer.ReactTestRenderer | undefined;

async function renderApp() {
  let created!: ReactTestRenderer.ReactTestRenderer;
  await ReactTestRenderer.act(async () => {
    created = ReactTestRenderer.create(<App />);
  });
  tree = created;
  await flush();
  return created;
}

async function pair(tree: ReactTestRenderer.ReactTestRenderer) {
  await ReactTestRenderer.act(() => {
    tree.root.findByProps({ testID: "pin-input" }).props.onChangeText("123456");
  });
  await ReactTestRenderer.act(async () => {
    await tree.root.findByProps({ testID: "pair-button" }).props.onPress();
  });
  await flush();
}

function screen(tree: { toJSON(): unknown }): string {
  return JSON.stringify(tree.toJSON());
}

beforeEach(() => {
  jest.clearAllMocks();
  (useWindowDimensions as jest.Mock).mockReturnValue({
    width: 390,
    height: 844,
    scale: 3,
    fontScale: 1,
  });
  (SecureStore as typeof SecureStore & { __reset: () => void }).__reset();
  (global as { WebSocket: { lastUrl: string } }).WebSocket.lastUrl = "";
  mockMixer();
});

afterEach(() => {
  ReactTestRenderer.act(() => {
    tree?.unmount();
  });
  tree = undefined;
});

test("renders a first-run pairing flow with manual setup kept as a fallback", async () => {
  const tree = await renderApp();
  const text = screen(tree);
  expect(text).toContain("on-air");
  expect(text).toContain("Pair your Mac");
  expect(text).toContain("This is a one-time setup");
  expect(text).toContain("Your Mac");
  expect(text).toContain("Search again");
  expect(tree.root.findAllByProps({ testID: "host-input" })).toHaveLength(0);
  expect(tree.root.findByProps({ testID: "pin-input" })).toBeTruthy();
  expect(tree.root.findByProps({ testID: "pair-button" })).toBeTruthy();

  ReactTestRenderer.act(() => {
    tree.root.findByProps({ testID: "manual-setup-button" }).props.onPress();
  });
  expect(tree.root.findByProps({ testID: "host-input" })).toBeTruthy();
  expect(tree.root.findByProps({ testID: "host-input" }).props.style.width).toBeUndefined();
  expect(tree.root.findByProps({ testID: "pin-input" }).props.style.width).toBeUndefined();
});

test("reveals the pairing code only after a Mac is selected", async () => {
  const pendingScan = deferred<(typeof STUDIO)[]>();
  mocked.discoverOnAir.mockReturnValueOnce(pendingScan.promise);
  const tree = await renderApp();

  expect(screen(tree)).toContain("Looking for your Mac");
  expect(tree.root.findAllByProps({ testID: "pin-input" })).toHaveLength(0);

  pendingScan.resolve([STUDIO]);
  await flush();

  expect(tree.root.findByProps({ testID: "pin-input" })).toBeTruthy();
  expect(screen(tree)).toContain("We'll remember this Mac");
});

test("adapts pairing and mixer controls to an unfolded two-pane layout", async () => {
  (useWindowDimensions as jest.Mock).mockReturnValue({
    width: 852,
    height: 883,
    scale: 2.4375,
    fontScale: 1,
  });
  const tree = await renderApp();
  expect(tree.root.findByProps({ testID: "pairing-wide-layout" })).toBeTruthy();

  await pair(tree);

  expect(tree.root.findByProps({ testID: "mixer-wide-layout" })).toBeTruthy();
  expect(tree.root.findByProps({ testID: "volume-slider-native" })).toBeTruthy();
  expect(tree.root.findByProps({ testID: "sound-settings-button" })).toBeTruthy();
});

test("keeps the full control path reachable in phone landscape", async () => {
  (useWindowDimensions as jest.Mock).mockReturnValue({
    width: 844,
    height: 390,
    scale: 3,
    fontScale: 1,
  });
  const tree = await renderApp();
  await pair(tree);

  expect(tree.root.findByProps({ testID: "mixer-landscape-layout" })).toBeTruthy();
  expect(tree.root.findByProps({ testID: "source-row" })).toBeTruthy();
  expect(tree.root.findByProps({ testID: "output-row" })).toBeTruthy();
  expect(tree.root.findByProps({ testID: "volume-slider-native" })).toBeTruthy();
  expect(tree.root.findByProps({ testID: "disconnect-button" })).toBeTruthy();
});

test("scan lists a discovered mixer and selects it", async () => {
  const tree = await renderApp();
  const discovered = tree.root.findByProps({ testID: "discovered-192.168.5.14" });
  expect(discovered).toBeTruthy();
  expect(discovered.props.accessibilityState).toEqual({ selected: true });
  expect(screen(tree)).toContain("studio");
  expect(screen(tree)).toContain("Ready for your code");
});

test("a late automatic scan cannot overwrite a manually entered desktop", async () => {
  const pendingScan = deferred<(typeof STUDIO)[]>();
  mocked.discoverOnAir.mockReturnValueOnce(pendingScan.promise);
  const tree = await renderApp();

  ReactTestRenderer.act(() => {
    tree.root.findByProps({ testID: "manual-setup-button" }).props.onPress();
  });
  ReactTestRenderer.act(() => {
    tree.root.findByProps({ testID: "host-input" }).props.onChangeText("192.168.5.77");
  });
  pendingScan.resolve([STUDIO]);
  await flush();

  expect(tree.root.findByProps({ testID: "host-input" }).props.value).toBe("192.168.5.77");
});

test("compact disc transport appears when a disc is loaded", async () => {
  mocked.getCd.mockResolvedValue({
    present: true,
    playing: true,
    track: 1,
    track_count: 2,
    title: "So What",
    album: "Kind of Blue",
    position_ms: 0,
    duration_ms: 180000,
  });
  mocked.controlCd.mockResolvedValue({
    present: true,
    playing: true,
    track: 2,
    track_count: 2,
    title: "Freddie Freeloader",
    album: "Kind of Blue",
    position_ms: 0,
    duration_ms: 180000,
  });
  const tree = await renderApp();
  await pair(tree);
  expect(tree.root.findByProps({ testID: "cd-transport" })).toBeTruthy();
  expect(tree.root.findByProps({ testID: "cd-track" })).toBeTruthy();
  expect(screen(tree)).toContain("So What");
  await ReactTestRenderer.act(async () => {
    await tree.root.findByProps({ testID: "cd-next" }).props.onPress();
  });
  expect(mocked.controlCd).toHaveBeenCalledWith("http://192.168.5.14:47990", "next", "onair-test");
});

test("pairing with the desktop PIN opens the full mixer", async () => {
  const tree = await renderApp();
  await pair(tree);
  expect(screen(tree)).toContain("Mock Monitor");
  expect(screen(tree)).toContain("Choose a speaker");
  expect(screen(tree)).toContain("Volume");
  expect(screen(tree)).toContain("Sound settings");
  ReactTestRenderer.act(() => {
    tree.root.findByProps({ testID: "output-row" }).props.onPress();
  });
  const outputText = screen(tree);
  expect(outputText).toContain("Mock Sonos");
  expect(outputText).toContain("Living Room AirPlay");
  expect(outputText).toContain("Locked HomePod");
  expect(outputText).toContain("Mock Bluetooth Speaker");
  expect(mocked.verifyPin).toHaveBeenCalledWith("http://192.168.5.14:47990", "123456");
  expect((global as { WebSocket: { lastUrl: string } }).WebSocket.lastUrl).toContain(
    "/api/ws?token=onair-test",
  );
  expect(SecureStore.setItemAsync).toHaveBeenCalledWith(
    "on-air.desktop-pairing.v1",
    JSON.stringify({ host: "192.168.5.14", token: "onair-test" }),
    expect.objectContaining({ keychainAccessible: SecureStore.WHEN_UNLOCKED_THIS_DEVICE_ONLY }),
  );
});

test("restores the paired desktop securely after an app restart", async () => {
  await SecureStore.setItemAsync(
    "on-air.desktop-pairing.v1",
    JSON.stringify({ host: "192.168.5.14", token: "saved-token" }),
  );
  jest.clearAllMocks();
  mockMixer();

  const tree = await renderApp();

  expect(screen(tree)).toContain("Mock Monitor");
  expect(screen(tree)).toContain("Choose a speaker");
  expect(mocked.verifyPin).not.toHaveBeenCalled();
  expect(mocked.discoverOnAir).not.toHaveBeenCalled();
  expect(mocked.listInputs).toHaveBeenCalledWith("http://192.168.5.14:47990", "saved-token");
});

test("source, destination, volume, EQ, and sample-rate drive the control API", async () => {
  const tree = await renderApp();
  await pair(tree);

  ReactTestRenderer.act(() => {
    tree.root.findByProps({ testID: "source-row" }).props.onPress();
  });
  await ReactTestRenderer.act(async () => {
    await tree.root
      .findByProps({ testID: "source-picker" })
      .props.onValueChange("PipeWire Sound Server");
  });
  expect(mocked.activateInput).toHaveBeenCalledWith(
    "http://192.168.5.14:47990",
    "PipeWire Sound Server",
    "onair-test",
  );

  ReactTestRenderer.act(() => {
    tree.root.findByProps({ testID: "output-row" }).props.onPress();
  });
  await ReactTestRenderer.act(async () => {
    await tree.root.findByProps({ testID: "output-sonos-uuid:mock-sonos" }).props.onPress();
  });
  expect(mocked.activateOutput).toHaveBeenCalledWith(
    "http://192.168.5.14:47990",
    "sonos",
    "uuid:mock-sonos",
    "onair-test",
  );

  jest.useFakeTimers();
  ReactTestRenderer.act(() => {
    tree.root.findByProps({ testID: "volume-slider-native" }).props.onValueChange(51);
  });
  await ReactTestRenderer.act(async () => {
    jest.advanceTimersByTime(180);
    await Promise.resolve();
  });
  jest.useRealTimers();
  expect(mocked.setVolume).toHaveBeenCalledWith("http://192.168.5.14:47990", 51, "onair-test");

  ReactTestRenderer.act(() => {
    tree.root.findByProps({ testID: "sound-settings-button" }).props.onPress();
  });
  ReactTestRenderer.act(() => {
    tree.root.findByProps({ label: "Equalizer" }).props.onOpenChange(true);
  });
  jest.useFakeTimers();
  ReactTestRenderer.act(() => {
    tree.root.findByProps({ testID: "eq-band-0-native" }).props.onValueChange(0.5);
  });
  await ReactTestRenderer.act(async () => {
    jest.advanceTimersByTime(180);
    await Promise.resolve();
  });
  jest.useRealTimers();
  expect(mocked.setEq).toHaveBeenCalledWith(
    "http://192.168.5.14:47990",
    [0.5, 0, 0, 0, 0],
    "onair-test",
  );

  ReactTestRenderer.act(() => {
    tree.root.findByProps({ label: "Sample rates" }).props.onOpenChange(true);
  });
  await ReactTestRenderer.act(async () => {
    tree.root.findByProps({ testID: "sample-rate-picker" }).props.onValueChange(48000);
  });
  expect(mocked.setSampleRate).toHaveBeenCalledWith(
    "http://192.168.5.14:47990",
    { input_hz: 48000 },
    "onair-test",
  );

  await ReactTestRenderer.act(async () => {
    tree.root.findByProps({ testID: "output-sample-rate-picker" }).props.onValueChange(48000);
  });
  expect(mocked.setSampleRate).toHaveBeenCalledWith(
    "http://192.168.5.14:47990",
    { output_hz: 48000 },
    "onair-test",
  );
});

test("native Expo sliders batch drag updates before calling the LAN API", async () => {
  const tree = await renderApp();
  await pair(tree);
  jest.useFakeTimers();
  try {
    mocked.setVolume.mockClear();
    ReactTestRenderer.act(() => {
      tree.root.findByProps({ testID: "volume-slider-native" }).props.onValueChange(73);
    });
    expect(mocked.setVolume).not.toHaveBeenCalled();
    await ReactTestRenderer.act(async () => {
      jest.advanceTimersByTime(180);
      await Promise.resolve();
    });
    expect(mocked.setVolume).toHaveBeenCalledWith("http://192.168.5.14:47990", 73, "onair-test");
  } finally {
    jest.useRealTimers();
  }
});

test("rapid EQ edits preserve every band and serialize network writes", async () => {
  const tree = await renderApp();
  await pair(tree);
  ReactTestRenderer.act(() => {
    tree.root.findByProps({ testID: "sound-settings-button" }).props.onPress();
  });
  ReactTestRenderer.act(() => {
    tree.root.findByProps({ label: "Equalizer" }).props.onOpenChange(true);
  });
  const firstWrite = deferred<void>();
  mocked.setEq.mockImplementationOnce(() => firstWrite.promise);
  mocked.setEq.mockClear();
  jest.useFakeTimers();
  try {
    ReactTestRenderer.act(() => {
      tree.root.findByProps({ testID: "eq-band-0-native" }).props.onValueChange(0.5);
      tree.root.findByProps({ testID: "eq-band-1-native" }).props.onValueChange(1);
    });
    await ReactTestRenderer.act(async () => {
      jest.advanceTimersByTime(180);
      await Promise.resolve();
    });
    expect(mocked.setEq).toHaveBeenCalledTimes(1);
    expect(mocked.setEq).toHaveBeenNthCalledWith(
      1,
      "http://192.168.5.14:47990",
      [0.5, 0, 0, 0, 0],
      "onair-test",
    );

    await ReactTestRenderer.act(async () => {
      firstWrite.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(mocked.setEq).toHaveBeenNthCalledWith(
      2,
      "http://192.168.5.14:47990",
      [0.5, 1, 0, 0, 0],
      "onair-test",
    );
  } finally {
    jest.useRealTimers();
  }
});

test("disconnect clears the saved pairing and returns to discovery", async () => {
  const tree = await renderApp();
  await pair(tree);
  ReactTestRenderer.act(() => {
    tree.root.findByProps({ testID: "disconnect-button" }).props.onPress();
  });
  expect(screen(tree)).toContain("Pair your Mac");
  expect(screen(tree)).not.toContain("Mock Sonos");
  expect(SecureStore.deleteItemAsync).toHaveBeenCalledWith("on-air.desktop-pairing.v1");
  await flush();
});

test("an expired response from an old connection cannot clear a new pairing", async () => {
  const tree = await renderApp();
  await pair(tree);
  const oldRefresh = deferred<string[]>();
  mocked.listInputs.mockImplementationOnce(() => oldRefresh.promise);
  ReactTestRenderer.act(() => {
    tree.root.findByProps({ testID: "more-button" }).props.onPress();
  });
  ReactTestRenderer.act(() => {
    tree.root.findByProps({ testID: "refresh-button" }).props.onPress();
  });

  ReactTestRenderer.act(() => {
    tree.root.findByProps({ testID: "disconnect-button" }).props.onPress();
  });
  await pair(tree);
  oldRefresh.reject(new client.HttpError("/api/inputs", 401));
  await flush();

  expect(screen(tree)).toContain("Mock Monitor");
  expect(screen(tree)).toContain("Choose a speaker");
});

test("unpaired AirPlay opens the PIN sheet then pairs and goes live", async () => {
  const tree = await renderApp();
  await pair(tree);

  ReactTestRenderer.act(() => {
    tree.root.findByProps({ testID: "output-row" }).props.onPress();
  });
  await ReactTestRenderer.act(() => {
    tree.root.findByProps({ testID: "output-airplay-ap-locked" }).props.onPress();
  });
  expect(tree.root.findByProps({ testID: "pair-sheet" })).toBeTruthy();
  expect(tree.root.findByProps({ testID: "device-pin-input" }).props.style.width).toBeUndefined();
  expect(mocked.activateOutput).not.toHaveBeenCalled();

  await ReactTestRenderer.act(() => {
    tree.root.findByProps({ testID: "device-pin-input" }).props.onChangeText("1111");
  });
  await ReactTestRenderer.act(async () => {
    await tree.root.findByProps({ testID: "device-pair-submit" }).props.onPress();
  });
  expect(mocked.pairAirplay).toHaveBeenCalledWith(
    "http://192.168.5.14:47990",
    "ap-locked",
    "1111",
    "onair-test",
  );
  expect(mocked.activateOutput).toHaveBeenCalledWith(
    "http://192.168.5.14:47990",
    "airplay",
    "ap-locked",
    "onair-test",
  );
});

test("unpaired Bluetooth confirms on the device then goes live", async () => {
  const tree = await renderApp();
  await pair(tree);

  ReactTestRenderer.act(() => {
    tree.root.findByProps({ testID: "output-row" }).props.onPress();
  });
  await ReactTestRenderer.act(() => {
    tree.root.findByProps({ testID: "output-bluetooth-bt-speaker" }).props.onPress();
  });
  expect(tree.root.findByProps({ testID: "pair-sheet" })).toBeTruthy();

  await ReactTestRenderer.act(async () => {
    await tree.root.findByProps({ testID: "device-pair-submit" }).props.onPress();
  });
  expect(mocked.pairBluetooth).toHaveBeenCalledWith(
    "http://192.168.5.14:47990",
    "bt-speaker",
    "onair-test",
  );
  expect(mocked.activateOutput).toHaveBeenCalledWith(
    "http://192.168.5.14:47990",
    "bluetooth",
    "bt-speaker",
    "onair-test",
  );
});
