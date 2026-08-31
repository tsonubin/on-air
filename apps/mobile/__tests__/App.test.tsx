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
    getAirplayMode: jest.fn(),
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
  getAirplayMode: jest.Mock;
  activateInput: jest.Mock;
  activateOutput: jest.Mock;
  setVolume: jest.Mock;
  setEq: jest.Mock;
  setSampleRate: jest.Mock;
  pairAirplay: jest.Mock;
  pairBluetooth: jest.Mock;
};

const STUDIO = { host: "192.168.5.14", port: 47990, name: "studio", version: "0.1.0" };

function mockMixer() {
  mocked.discoverOnAir.mockResolvedValue([STUDIO]);
  mocked.fetchStatus.mockResolvedValue({ status: "ok", version: "0.1.0" });
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
  mocked.getSampleRate.mockResolvedValue({
    sample_rate_hz: 44100,
    input: { sample_rate_hz: 44100, supported_hz: [44100, 48000] },
    output: { sample_rate_hz: 44100, supported_hz: [44100, 48000] },
  });
  mocked.getAirplayMode.mockResolvedValue("owntone");
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
  (global as { WebSocket: { lastUrl: string } }).WebSocket.lastUrl = "";
  mockMixer();
});

afterEach(() => {
  ReactTestRenderer.act(() => {
    tree?.unmount();
  });
  tree = undefined;
});

test("renders the find-desktop pairing card", async () => {
  const tree = await renderApp();
  const text = screen(tree);
  expect(text).toContain("on-air remote");
  expect(text).toContain("ONAIR");
  expect(text).toContain("Find desktop");
  expect(text).toContain("Scan LAN");
  expect(tree.root.findByProps({ testID: "host-input" })).toBeTruthy();
  expect(tree.root.findByProps({ testID: "pin-input" })).toBeTruthy();
  expect(tree.root.findByProps({ testID: "pair-button" })).toBeTruthy();
});

test("scan lists a discovered mixer and selects it", async () => {
  const tree = await renderApp();
  expect(tree.root.findByProps({ testID: "discovered-192.168.5.14" })).toBeTruthy();
  expect(screen(tree)).toContain("studio");
  expect(screen(tree)).toContain("192.168.5.14");
  expect(tree.root.findByProps({ testID: "host-input" }).props.value).toBe("192.168.5.14");
});

test("pairing with the desktop PIN opens the full mixer", async () => {
  const tree = await renderApp();
  await pair(tree);
  const text = screen(tree);
  expect(text).toContain("paired");
  expect(text).toContain("Mock Monitor");
  expect(text).toContain("Mock Sonos");
  expect(text).toContain("Living Room AirPlay");
  expect(text).toContain("Locked HomePod");
  expect(text).toContain("Mock Bluetooth Speaker");
  expect(text).toContain("volume");
  expect(text).toContain('"60"');
  expect(mocked.verifyPin).toHaveBeenCalledWith("http://192.168.5.14:47990", "123456");
  expect((global as { WebSocket: { lastUrl: string } }).WebSocket.lastUrl).toContain(
    "/api/ws?token=onair-test",
  );
});

test("source, destination, volume, EQ, and sample-rate drive the control API", async () => {
  const tree = await renderApp();
  await pair(tree);

  await ReactTestRenderer.act(async () => {
    await tree.root.findByProps({ testID: "input-PipeWire Sound Server" }).props.onPress();
  });
  expect(mocked.activateInput).toHaveBeenCalledWith(
    "http://192.168.5.14:47990",
    "PipeWire Sound Server",
    "onair-test",
  );

  await ReactTestRenderer.act(async () => {
    await tree.root.findByProps({ testID: "output-sonos-uuid:mock-sonos" }).props.onPress();
  });
  expect(mocked.activateOutput).toHaveBeenCalledWith(
    "http://192.168.5.14:47990",
    "sonos",
    "uuid:mock-sonos",
    "onair-test",
  );

  await ReactTestRenderer.act(async () => {
    await tree.root.findByProps({ testID: "volume-slider-up" }).props.onPress();
  });
  expect(mocked.setVolume).toHaveBeenCalledWith("http://192.168.5.14:47990", 51, "onair-test");

  await ReactTestRenderer.act(async () => {
    await tree.root.findByProps({ testID: "eq-band-0-up" }).props.onPress();
  });
  expect(mocked.setEq).toHaveBeenCalledWith(
    "http://192.168.5.14:47990",
    [0.5, 0, 0, 0, 0],
    "onair-test",
  );

  await ReactTestRenderer.act(async () => {
    tree.root.findByProps({ testID: "sample-rate-48000" }).props.onPress();
  });
  expect(mocked.setSampleRate).toHaveBeenCalledWith(
    "http://192.168.5.14:47990",
    { input_hz: 48000 },
    "onair-test",
  );

  await ReactTestRenderer.act(async () => {
    tree.root.findByProps({ testID: "output-sample-rate-48000" }).props.onPress();
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

test("disconnect returns to discovery without restarting the app", async () => {
  const tree = await renderApp();
  await pair(tree);
  ReactTestRenderer.act(() => {
    tree.root.findByProps({ testID: "disconnect-button" }).props.onPress();
  });
  expect(screen(tree)).toContain("Find desktop");
  expect(screen(tree)).not.toContain("Mock Sonos");
});

test("unpaired AirPlay opens the PIN sheet then pairs and goes live", async () => {
  const tree = await renderApp();
  await pair(tree);

  await ReactTestRenderer.act(() => {
    tree.root.findByProps({ testID: "output-airplay-ap-locked" }).props.onPress();
  });
  expect(tree.root.findByProps({ testID: "pair-sheet" })).toBeTruthy();
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
