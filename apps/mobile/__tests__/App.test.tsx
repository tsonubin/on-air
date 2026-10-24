import type { CdStatus, OutputInfo } from "@on-air/api-types";
import * as client from "@on-air/control-client";
import { FakeSocket } from "@on-air/control-client/testing";
import * as expoRouter from "expo-router";
import * as SecureStore from "expo-secure-store";
import { Keyboard, Platform, useWindowDimensions } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import ReactTestRenderer, {
  type ReactTestInstance,
  type TestRendererOptions,
} from "react-test-renderer";
import RootLayout from "../app/_layout";
import { keysFitInRow } from "../src/mixer-home";

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
// `react-native` is mapped to __mocks__/react-native.js, whose Keyboard adds `__emit`.
const keyboard = Keyboard as typeof Keyboard & { __emit(type: string, event: unknown): void };
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

async function renderApp(options?: TestRendererOptions) {
  await ReactTestRenderer.act(async () => {
    tree = ReactTestRenderer.create(<RootLayout />, options);
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

/** Moves a fader by one accessibility step, as VoiceOver's swipe up / down does. */
function stepFader(id: string, action: "increment" | "decrement", times = 1) {
  for (let i = 0; i < times; i += 1) {
    ReactTestRenderer.act(() =>
      host(id).props.onAccessibilityAction({ nativeEvent: { actionName: action } }),
    );
  }
}

/** The first sentence of the LED window's summary: what the status pill used to announce. */
const announced = () => (host("led-window").props.accessibilityLabel as string).split(". ")[0];

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

/** A native-stack toolbar button, found by its spoken label (toolbar items take no testID). */
function toolbarButton(label: string): ReactTestInstance {
  return (tree as ReactTestRenderer.ReactTestRenderer).root.find(
    (node) =>
      (node.type as unknown) === "NativeToolbarButton" && node.props.accessibilityLabel === label,
  );
}

/** Taps the Connection screen's top-right refresh button. */
async function pressRefresh() {
  const button = toolbarButton("Refresh connection");
  if (button.props.disabled) throw new Error("refresh is disabled");
  await ReactTestRenderer.act(async () => {
    await button.props.onPress();
  });
  await flush();
}

beforeEach(() => {
  jest.clearAllMocks();
  routerMock.__reset();
  store.__reset();
  FakeSocket.reset();
  (globalThis as { WebSocket: unknown }).WebSocket = FakeSocket;
  jest.mocked(useWindowDimensions).mockReturnValue({
    width: 390,
    height: 844,
    scale: 3,
    fontScale: 1,
  });
  jest.mocked(useSafeAreaInsets).mockReturnValue({ top: 47, right: 0, bottom: 34, left: 0 });
  mockDesktop();
});

afterEach(() => {
  ReactTestRenderer.act(() => tree?.unmount());
  tree = undefined;
  keyboard.__emit("keyboardDidHide", {});
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
  expect(announced()).toBe("Status: Ready");
  expect(FakeSocket.last?.url).toBe(`ws://192.168.5.14:47990/api/ws?token=${TOKEN}`);
  expect(host("volume-slider").props.accessibilityState).toEqual({ disabled: true });

  // Choose a source.
  await press("source-key");
  await ReactTestRenderer.act(async () => {
    await host("source-picker").props.onValueChange("PipeWire Sound Server");
  });
  await flush();
  expect(api.activateInput).toHaveBeenCalledWith(BASE, "PipeWire Sound Server", TOKEN);
  expect(exists("source-sheet")).toBe(false);

  // Choose a speaker; the desktop's event marks it live without a full refetch.
  await press("speaker-key");
  expect(text()).toContain("Living Room AirPlay");
  expect(text()).toContain("Stereo pair · AirPlay");
  const listsBefore = api.listInputs.mock.calls.length;
  api.getActiveOutput.mockResolvedValue({
    transport: "sonos",
    device_id: "uuid:mock-sonos",
    device_name: "Mock Sonos",
  });
  await press("output-sonos-uuid:mock-sonos");
  expect(api.activateOutput).toHaveBeenCalledWith(BASE, "sonos", "uuid:mock-sonos", TOKEN);
  expect(exists("output-sheet")).toBe(false);
  ReactTestRenderer.act(() => FakeSocket.last?.open());
  ReactTestRenderer.act(() =>
    FakeSocket.last?.send({
      type: "OutputStateChanged",
      transport: "sonos",
      device_name: "Mock Sonos",
      active: true,
    }),
  );
  expect(announced()).toBe("Audio is live");
  expect(text()).toContain("Audio is live");
  expect(api.listInputs.mock.calls.length).toBe(listsBefore);

  // Volume commits after the drag pauses; the LED follows the fader before that.
  jest.useFakeTimers();
  stepFader("volume-slider", "increment");
  expect(api.setVolume).not.toHaveBeenCalled();
  expect(numbersShown()).toContain(55);
  await ReactTestRenderer.act(async () => {
    jest.advanceTimersByTime(180);
  });
  expect(api.setVolume).toHaveBeenCalledWith(BASE, 55, TOKEN);

  // Equalizer, straight from its key: two quick band edits keep both values and serialise.
  await press("eq-key");
  expect(host("eq-band-0").props.accessibilityLabel).toBe("Bass, 60 hertz");
  stepFader("eq-band-0", "increment");
  stepFader("eq-band-1", "increment", 2);
  await ReactTestRenderer.act(async () => {
    jest.advanceTimersByTime(180);
  });
  jest.useRealTimers();
  await flush();
  expect(api.setEq).toHaveBeenLastCalledWith(BASE, [1, 2, 0, 0, 0], TOKEN);

  // Audio format, from its own key on the mixer.
  ReactTestRenderer.act(() => expoRouter.router.back());
  await press("format-key");
  await ReactTestRenderer.act(async () => {
    await host("output-sample-rate-picker").props.onValueChange(48000);
  });
  expect(api.setSampleRate).toHaveBeenCalledWith(BASE, { output_hz: 48000 }, TOKEN);

  // One back reaches the mixer; Connection shows the paired host, and Forget returns to pairing.
  await press("native-back");
  expect(exists("mixer-screen")).toBe(true);
  await press("more-button");
  expect(host("paired-host").props.children).toBe(STUDIO.host);
  expect(text()).toContain("Connected to desktop");
  await press("disconnect-menu-button");
  expect(text()).toContain("Pair your Mac");
  expect(store.deleteItemAsync).toHaveBeenCalledWith(PAIRING_KEY);
  expect(FakeSocket.last?.closed).toBe(true);
});

/** Numbers rendered in host Text nodes (the volume readout among them). */
/** Numbers visible on screen, including LED readouts (their ghost "8" segments are hidden). */
function numbersShown(): number[] {
  return (tree as ReactTestRenderer.ReactTestRenderer).root
    .findAll(
      (node) =>
        (node.type as unknown) === "Text" &&
        !node.props.accessibilityElementsHidden &&
        (typeof node.props.children === "number" ||
          (typeof node.props.children === "string" && /^\d+$/.test(node.props.children))),
    )
    .map((node) => Number(node.props.children));
}

test("a failed volume write rolls the readout back to what the desktop holds", async () => {
  api.getActiveOutput.mockResolvedValue({
    transport: "sonos",
    device_id: "uuid:mock-sonos",
    device_name: "Mock Sonos",
    state: "live",
  });
  await renderApp();
  await pairWith();
  expect(announced()).toBe("Audio is live");
  expect(numbersShown()).toContain(50);
  api.setVolume.mockRejectedValue(
    new client.HttpError("/api/outputs/volume", 502, '{"error":"","code":"transport_unreachable"}'),
  );
  jest.useFakeTimers();
  stepFader("volume-slider", "increment");
  await ReactTestRenderer.act(async () => {
    jest.advanceTimersByTime(180);
  });
  // The LED follows the fader, so it settles back to the desktop's value once the write fails.
  await ReactTestRenderer.act(async () => {
    jest.advanceTimersByTime(300);
  });
  jest.useRealTimers();
  await flush();
  expect(api.setVolume).toHaveBeenCalledWith(BASE, 55, TOKEN);
  expect(numbersShown()).toContain(50);
  expect(numbersShown()).not.toContain(55);
  expect(text()).toContain("speaker did not answer");
  // Errors read as plain faceplate text under the window, with no box.
  const notice = flat(host("mixer-error").props.style);
  expect(notice.backgroundColor).toBeUndefined();
  expect(notice.borderWidth).toBeUndefined();
});

test("a speaker that is still starting is not shown as live; a failed one can be chosen again", async () => {
  api.getActiveOutput.mockResolvedValue({
    transport: "sonos",
    device_id: "uuid:mock-sonos",
    device_name: "Mock Sonos",
    state: "starting",
  });
  await renderApp();
  await pairWith();
  expect(announced()).toBe("Status: Connecting speaker");
  expect(host("volume-slider").props.accessibilityState).toEqual({ disabled: true });
  expect(host("led-status").props.children).toBe("CONNECTING");

  api.getActiveOutput.mockResolvedValue({
    transport: "sonos",
    device_id: "uuid:mock-sonos",
    device_name: "Mock Sonos",
    state: "failed",
  });
  await press("more-button");
  await pressRefresh();
  await press("native-back");
  expect(announced()).toBe("Status: Speaker failed");
  expect(host("led-status").props.children).toBe("FAILED");
  expect(host("speaker-failed").props.children).toBe("Speaker failed. Choose it again.");
  await press("speaker-key");
  expect(text()).toContain("Failed · Retry");
  await press("output-sonos-uuid:mock-sonos");
  expect(api.activateOutput).toHaveBeenCalledWith(BASE, "sonos", "uuid:mock-sonos", TOKEN);
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

test("the pairing form moves up out of the keyboard's way", async () => {
  const original = Platform.OS;
  try {
    // iOS insets the ScrollView itself and scrolls the focused field above the keyboard.
    await renderApp();
    expect(host("pairing-scroll").props.automaticallyAdjustKeyboardInsets).toBe(true);
    expect(host("pairing-screen").props.behavior).toBeUndefined();

    // Edge-to-edge Android does not resize the window for the keyboard, so the screen pads itself.
    ReactTestRenderer.act(() => tree?.unmount());
    (Platform as { OS: string }).OS = "android";
    await renderApp();
    expect(host("pairing-screen").props.behavior).toBe("padding");
  } finally {
    (Platform as { OS: string }).OS = original;
  }
});

// An iPhone 17 Pro on iOS 27: the number pad's frame starts at y=524, and its top strip is a
// see-through bar with "Done" that takes every tap in it, so the code panel must end above it.
const NUMBER_PAD = { endCoordinates: { screenX: 0, screenY: 524, width: 402, height: 350 } };

/** Stand-ins for the native pairing views: where the code panel sits and what got scrolled. */
function pairingViews() {
  const views = { scrollTo: jest.fn(), pinFocused: false, panelBottom: 580 };
  const options: TestRendererOptions = {
    createNodeMock: (element) => {
      switch ((element.props as { testID?: string }).testID) {
        case "pairing-scroll":
          return { scrollTo: views.scrollTo };
        case "pin-input":
          return { isFocused: () => views.pinFocused };
        case "pairing-code":
          return {
            measureInWindow: (callback: (x: number, y: number, w: number, h: number) => void) =>
              callback(20, views.panelBottom - 240, 362, 240),
          };
        default:
          return null;
      }
    },
  };
  return { views, options };
}

function scrolledTo(y: number) {
  ReactTestRenderer.act(() =>
    host("pairing-scroll").props.onScroll({ nativeEvent: { contentOffset: { x: 0, y } } }),
  );
}

function showKeyboard(event: unknown = NUMBER_PAD) {
  ReactTestRenderer.act(() => keyboard.__emit("keyboardDidShow", event));
}

test("the code panel scrolls clear of the number pad and its Done bar", async () => {
  const { views, options } = pairingViews();
  await renderApp(options);
  scrolledTo(30);

  // A keyboard for another field leaves the screen where it is.
  showKeyboard();
  expect(views.scrollTo).not.toHaveBeenCalled();

  // iOS follows every focus change with keyboardDidShow, so the focus itself waits for it.
  views.pinFocused = true;
  ReactTestRenderer.act(() => host("pin-input").props.onFocus());
  expect(views.scrollTo).not.toHaveBeenCalled();

  // 30pt already scrolled, plus the panel's 56pt overlap, plus 12pt of room above the bar.
  showKeyboard();
  expect(views.scrollTo).toHaveBeenCalledWith({ y: 98, animated: true });
  views.scrollTo.mockClear();

  // A frame with no height covers nothing.
  showKeyboard({ endCoordinates: { screenX: 0, screenY: 0, width: 0, height: 0 } });
  expect(views.scrollTo).not.toHaveBeenCalled();

  // A panel that already clears the keyboard stays put.
  views.panelBottom = 500;
  showKeyboard();
  expect(views.scrollTo).not.toHaveBeenCalled();
});

test("the code panel stays clear of the number pad when manual setup opens above it", async () => {
  const { views, options } = pairingViews();
  await renderApp(options);
  views.pinFocused = true;
  views.panelBottom = 500;
  showKeyboard();
  scrolledTo(98);

  // The address card pushes the panel 88pt down, back under the keyboard, with no keyboard event.
  await press("manual-setup-button");
  views.panelBottom = 588;
  ReactTestRenderer.act(() => host("pairing-code").props.onLayout());
  expect(views.scrollTo).toHaveBeenCalledWith({ y: 174, animated: true });

  // With the keyboard gone there is nothing to clear.
  views.scrollTo.mockClear();
  ReactTestRenderer.act(() => keyboard.__emit("keyboardDidHide", {}));
  ReactTestRenderer.act(() => host("pairing-code").props.onLayout());
  expect(views.scrollTo).not.toHaveBeenCalled();
});

// A Pixel 8 on Android 16: the number pad, suggestion strip included, starts at y=571.
const ANDROID_NUMBER_PAD = {
  endCoordinates: { screenX: 0, screenY: 571, width: 411, height: 295 },
};

async function onAndroid(check: () => Promise<void>) {
  const original = Platform.OS;
  (Platform as { OS: string }).OS = "android";
  try {
    await check();
  } finally {
    (Platform as { OS: string }).OS = original;
  }
}

test("on Android the code panel clears the number pad once the screen has padded itself", () =>
  onAndroid(async () => {
    const { views, options } = pairingViews();
    views.pinFocused = true;
    views.panelBottom = 807;
    await renderApp(options);

    // keyboardDidShow comes before the padding, so the scroll it starts stops at the old scroll
    // range. The ScrollView shrinking for the keyboard runs the clearance again.
    showKeyboard(ANDROID_NUMBER_PAD);
    views.scrollTo.mockClear();
    ReactTestRenderer.act(() => host("pairing-scroll").props.onLayout());
    // The panel's 236pt overlap plus 12pt of room above the number pad.
    expect(views.scrollTo).toHaveBeenCalledWith({ y: 248, animated: true });
  }));

test("on Android the code panel clears the keyboard when the code field takes focus under it", () =>
  onAndroid(async () => {
    const { views, options } = pairingViews();
    views.panelBottom = 807;
    await renderApp(options);
    showKeyboard(ANDROID_NUMBER_PAD);
    expect(views.scrollTo).not.toHaveBeenCalled();

    // The address keyboard is as tall as the number pad, so moving on with "next" sends no
    // keyboard event and changes no layout. Only the focus says the code field is now in use.
    views.pinFocused = true;
    ReactTestRenderer.act(() => host("pin-input").props.onFocus());
    expect(views.scrollTo).toHaveBeenCalledWith({ y: 248, animated: true });
  }));

test("an expired pairing returns to pairing with the desktop still selected", async () => {
  await renderApp();
  await pairWith();
  api.fetchStatus.mockRejectedValueOnce(new Error("Timeout"));
  api.listInputs.mockRejectedValueOnce(
    new client.HttpError("/api/inputs", 401, '{"error":"","code":"not_paired"}'),
  );
  await press("more-button");
  await pressRefresh();
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
  await pressRefresh();
  expect(text()).toContain("Connected to desktop");
  expect(text()).toContain("desktop service is paused");
  await press("native-back");
  expect(announced()).toBe("Desktop service is paused");
  expect(host("eq-key").props.onPress).toBeUndefined();
  expect(host("format-key").props.onPress).toBeUndefined();
  expect(store.deleteItemAsync).not.toHaveBeenCalled();
});

test("the mixer is one faceplate: no bordered boxes anywhere on home", async () => {
  await renderApp();
  await pairWith();
  const boxes = host("mixer-screen").findAll(
    (n) =>
      ["View", "Pressable"].includes(n.type as string) &&
      Number(flat(n.props.style).borderWidth ?? 0) > 0,
  );
  // The ONAIR sign is a lamp housing, not a group box; it is the only bordered shape left.
  expect(boxes.map((b) => b.props.testID)).toEqual(["mixer-sign"]);
  expect(exists("faceplate-finish")).toBe(true);
});

test("the LED window's format corner shows the output sample rate", async () => {
  await renderApp();
  await pairWith();
  expect(host("led-format").props.children).toBe("44.1K · ST");
  expect(host("led-source").props.children).toBe("Mock Monitor");
});

test("restoring a saved pairing shows the faceplate while it loads", async () => {
  // Hold the keychain read open so the restoring screen stays up to be inspected.
  let release!: (saved: string | null) => void;
  store.getItemAsync.mockReturnValueOnce(new Promise((resolve) => (release = resolve)));
  await ReactTestRenderer.act(async () => {
    tree = ReactTestRenderer.create(<RootLayout />);
  });
  expect(exists("pairing-restore")).toBe(true);
  expect(exists("faceplate-finish")).toBe(true);
  await ReactTestRenderer.act(async () => {
    release(JSON.stringify({ host: "192.168.5.14", port: 47990, token: "saved-token" }));
  });
  await flush();
  expect(exists("mixer-screen")).toBe(true);
});

test("the four mixer keys open the sheets and the sound screens", async () => {
  await renderApp();
  await pairWith();
  // The Sound menu and the home Disconnect are gone: keys go straight to each screen.
  expect(exists("sound-settings-button")).toBe(false);
  expect(exists("disconnect-button")).toBe(false);
  await press("source-key");
  expect(exists("source-sheet")).toBe(true);
});

test("the EQ key says Custom with an amber dot when any band is moved", async () => {
  api.getEq.mockResolvedValue([0, 2, 0, 0, 0]);
  await renderApp();
  await pairWith();
  expect(host("eq-key").props.accessibilityValue).toEqual({ text: "Custom" });
  expect(exists("eq-key-indicator")).toBe(true);
});

test("the EQ key says Flat with no dot when every band is at 0 dB", async () => {
  await renderApp();
  await pairWith();
  expect(host("eq-key").props.accessibilityValue).toEqual({ text: "Flat" });
  expect(exists("eq-key-indicator")).toBe(false);
});

test("keys wrap into a 2 × 2 grid at the largest text size", async () => {
  jest
    .mocked(useWindowDimensions)
    .mockReturnValue({ width: 390, height: 844, scale: 3, fontScale: 1.6 });
  await renderApp();
  await pairWith();
  expect(flat(host("eq-key").props.style)).toMatchObject({ flexBasis: "48%" });
});

test("keys wrap once their captions would not fit, as at iOS XXL text on a 393 pt phone", async () => {
  // Found in review: SPEAKER truncated at 1.235× because the grid waited for 1.3×.
  jest
    .mocked(useWindowDimensions)
    .mockReturnValue({ width: 393, height: 852, scale: 3, fontScale: 1.235 });
  await renderApp();
  await pairWith();
  expect(flat(host("eq-key").props.style)).toMatchObject({ flexBasis: "48%" });
});

test("keys measure their row: a narrow column wraps even at the default text size", async () => {
  await renderApp();
  await pairWith();
  ReactTestRenderer.act(() =>
    host("mixer-keys").props.onLayout({
      nativeEvent: { layout: { x: 0, y: 0, width: 312, height: 56 } },
    }),
  );
  expect(flat(host("eq-key").props.style)).toMatchObject({ flexBasis: "48%" });
});

test("before the row is measured, the keys' width estimate leaves out a side rail", async () => {
  // Found on the folded iPhone Duo: the estimate ignored the 84 pt rail, so the
  // first frame put four keys in one row at the largest text size.
  jest.mocked(useSafeAreaInsets).mockReturnValue({ top: 0, right: 84, bottom: 34, left: 0 });
  jest
    .mocked(useWindowDimensions)
    .mockReturnValue({ width: 466, height: 678, scale: 3, fontScale: 3.571 });
  await renderApp();
  await pairWith();
  expect(flat(host("eq-key").props.style)).toMatchObject({ flexBasis: "48%" });
});

test("switching between one row and the grid mounts a fresh key row", async () => {
  // React Native 0.88 doesn't re-lay out a wrapping row when its keys' flex
  // basis changes in place (seen on the iPhone Duo when the text size changed).
  await renderApp();
  await pairWith();
  setWindow(390, 844, 1);
  const row = host("mixer-keys");
  setWindow(390, 844, 1.1);
  expect(host("mixer-keys")).toBe(row);
  setWindow(390, 844, 1.6);
  const grid = host("mixer-keys");
  expect(grid).not.toBe(row);
  expect(flat(host("eq-key").props.style)).toMatchObject({ flexBasis: "48%" });
  setWindow(390, 844, 1);
  expect(host("mixer-keys")).not.toBe(grid);
  expect(flat(host("eq-key").props.style)).toMatchObject({ flexBasis: 0 });
});

test("keys share one row at the default text size", async () => {
  await renderApp();
  await pairWith();
  expect(flat(host("eq-key").props.style)).toMatchObject({ flexGrow: 1, flexBasis: 0 });
});

test("unpaired AirPlay asks for the speaker code, then pairs and connects", async () => {
  await renderApp();
  await pairWith();
  await press("speaker-key");
  await press("output-airplay-ap-locked");
  expect(exists("output-sheet")).toBe(false);
  expect(exists("pair-sheet")).toBe(true);
  expect(api.activateOutput).not.toHaveBeenCalled();
  // Red is only for live state; the speaker-code field sits in the faceplate palette.
  expect(flat(host("device-pin-input").props.style).borderColor).toBe("#5c4a32");
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
  // Transport buttons are faceplate keys, like the desktop's.
  expect(flat(host("cd-play").props.style)).toMatchObject({
    backgroundColor: "#1d1a15",
    experimental_backgroundImage: "linear-gradient(180deg, #2a261f, #1d1a15)",
  });
  await press("cd-next");
  expect(api.controlCd).toHaveBeenCalledWith(BASE, "next", TOKEN);
  expect(text()).toContain("Freddie Freeloader");
});

/** Sets the window, as a fold, unfold, rotation or split view does, and re-renders. */
function setWindow(width: number, height: number, fontScale = 1) {
  jest.mocked(useWindowDimensions).mockReturnValue({ width, height, scale: 3, fontScale });
  ReactTestRenderer.act(() => tree?.update(<RootLayout />));
}

const touch = (x: number, y = 0) => ({ nativeEvent: { pageX: x, pageY: y } });

test("layouts: mixer + EQ when unfolded, the two-column mixer in phone landscape", async () => {
  jest.mocked(useWindowDimensions).mockReturnValue({
    width: 852,
    height: 883,
    scale: 2.4375,
    fontScale: 1,
  });
  await renderApp();
  expect(exists("pairing-wide-layout")).toBe(true);
  await pairWith();
  expect(host("mixer-panes").findAll((n) => n.props.testID === "eq-pane").length).toBeGreaterThan(
    0,
  );
  expect(exists("mixer-pane-layout")).toBe(true);
  expect(exists("eq-key")).toBe(false);
  for (const id of ["source-key", "speaker-key", "volume-slider", "format-key"]) {
    expect(exists(id)).toBe(true);
  }

  setWindow(844, 390);
  expect(exists("mixer-landscape-layout")).toBe(true);
  expect(exists("eq-pane")).toBe(false);
  for (const id of ["source-key", "speaker-key", "volume-slider", "eq-key", "format-key"]) {
    expect(exists(id)).toBe(true);
  }
});

test("unfolding shows the EQ beside the mixer; folding puts it away", async () => {
  await renderApp();
  await pairWith();
  setWindow(375, 812); // folded: the outer screen
  expect(exists("eq-pane")).toBe(false);
  expect(exists("eq-key")).toBe(true);
  setWindow(669, 951); // unfolded
  expect(exists("eq-pane")).toBe(true);
  expect(exists("eq-key")).toBe(false);
  setWindow(375, 812);
  expect(exists("eq-pane")).toBe(false);
  expect(exists("eq-key")).toBe(true);
});

test("the EQ pane writes through the same session calls", async () => {
  await renderApp();
  await pairWith();
  setWindow(669, 951);
  jest.useFakeTimers();
  stepFader("eq-band-2", "increment");
  await ReactTestRenderer.act(async () => {
    jest.advanceTimersByTime(180);
  });
  jest.useRealTimers();
  await flush();
  expect(api.setEq).toHaveBeenLastCalledWith(BASE, [0, 0, 1, 0, 0], TOKEN);
  await press("eq-pane-reset");
  expect(api.setEq).toHaveBeenLastCalledWith(BASE, [0, 0, 0, 0, 0], TOKEN);
});

test("a fold mid-drag sends no stray commit and keeps the volume fader", async () => {
  api.getActiveOutput.mockResolvedValue({
    transport: "sonos",
    device_id: "uuid:mock-sonos",
    device_name: "Mock Sonos",
    state: "live",
  });
  await renderApp();
  await pairWith();
  setWindow(669, 951);
  const fader = host("volume-slider");
  jest.useFakeTimers();
  ReactTestRenderer.act(() => fader.props.onResponderGrant(touch(0)));
  setWindow(375, 812); // folded while the finger is down
  expect(host("volume-slider")).toBe(fader);
  await ReactTestRenderer.act(async () => {
    jest.advanceTimersByTime(500);
  });
  jest.useRealTimers();
  expect(api.setVolume).not.toHaveBeenCalled();
});

test("a narrow split view falls back to the phone layout", async () => {
  await renderApp();
  await pairWith();
  setWindow(320, 900);
  expect(exists("mixer-phone-layout")).toBe(true);
  expect(exists("eq-pane")).toBe(false);
  expect(exists("eq-key")).toBe(true);
});

test("a paused desktop disables the EQ pane", async () => {
  await renderApp();
  await pairWith();
  api.fetchStatus.mockResolvedValue({ status: "ok", version: "0.1.0", service_enabled: false });
  for (const read of [api.listInputs, api.listOutputs, api.getEq, api.getVolume]) {
    read.mockRejectedValue(
      new client.HttpError("/api/x", 503, '{"error":"","code":"service_paused"}'),
    );
  }
  await press("more-button");
  await pressRefresh();
  await press("native-back");
  setWindow(669, 951);
  expect(host("eq-band-0").props.accessibilityState).toEqual({ disabled: true });
  expect(host("eq-pane-reset").props.onPress).toBeUndefined();
});

test("the three-key row wraps at the largest text in a narrow pane", async () => {
  await renderApp();
  await pairWith();
  setWindow(669, 951, 1.6);
  // The Duo's ~293 pt pane still fits three keys at the 1.3× cap; a narrower one must wrap.
  ReactTestRenderer.act(() =>
    host("mixer-keys").props.onLayout({
      nativeEvent: { layout: { x: 0, y: 0, width: 293, height: 56 } },
    }),
  );
  expect(flat(host("source-key").props.style)).toMatchObject({ flexBasis: 0 });
  ReactTestRenderer.act(() =>
    host("mixer-keys").props.onLayout({
      nativeEvent: { layout: { x: 0, y: 0, width: 260, height: 56 } },
    }),
  );
  expect(flat(host("source-key").props.style)).toMatchObject({ flexBasis: "48%" });
  expect(keysFitInRow(290, 1, 3)).toBe(true);
  expect(keysFitInRow(200, 1, 3)).toBe(false);
});

/** The content style of the scroll view that holds a host element. */
function scrollContentOf(id: string): Record<string, unknown> {
  let node = host(id).parent;
  while (node && node.props.contentContainerStyle === undefined) node = node.parent;
  return flat(node?.props.contentContainerStyle);
}

test("the mixer and its EQ pane keep clear of a side rail", async () => {
  jest.mocked(useSafeAreaInsets).mockReturnValue({ top: 0, right: 59, bottom: 21, left: 59 });
  await renderApp();
  await pairWith();
  expect(scrollContentOf("mixer-phone-layout")).toMatchObject({
    paddingLeft: 59,
    paddingRight: 59,
  });
  setWindow(951, 669); // unfolded, sideways: the rail sits on a side
  expect(flat(host("eq-pane-scroll").props.contentContainerStyle)).toMatchObject({
    paddingRight: 59,
  });
  expect(scrollContentOf("mixer-pane-layout")).toMatchObject({
    paddingLeft: 59,
  });
});

test("pairing keeps clear of a side rail", async () => {
  jest.mocked(useSafeAreaInsets).mockReturnValue({ top: 0, right: 59, bottom: 21, left: 59 });
  await renderApp();
  expect(flat(host("pairing-scroll").props.contentContainerStyle)).toMatchObject({
    paddingLeft: 59,
    paddingRight: 59,
  });
});

test("unfolding while the EQ screen is open keeps it; Back returns to the two panes", async () => {
  await renderApp();
  await pairWith();
  setWindow(375, 812);
  await press("eq-key");
  setWindow(669, 951);
  expect(exists("equalizer-screen")).toBe(true);
  expect(exists("eq-pane")).toBe(false);
  await press("native-back");
  expect(exists("mixer-screen")).toBe(true);
  expect(exists("eq-pane")).toBe(true);
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
  const pill = host("led-window");
  expect(pill.props.accessible).toBe(true);
  expect(pill.props.accessibilityRole).toBe("text");
  // RN content must not be nested directly in a SwiftUI Host.
  let parent = root.findByProps({ testID: "mixer-screen" }).parent;
  while (parent) {
    expect(parent.type).not.toBe("ExpoHost");
    parent = parent.parent;
  }
});

// Style props are arbitrarily nested arrays of objects; flatten like StyleSheet.flatten.
const flat = (style: any): Record<string, unknown> =>
  Array.isArray(style) ? Object.assign({}, ...style.map(flat)) : (style ?? {});
const textsIn = (node: ReactTestInstance) =>
  node.findAll((n) => (n.type as unknown) === "Text" && typeof n.props.children === "string");

test("retro header: the sign stays unlit when ready and lights while live", async () => {
  await renderApp();
  await pairWith();
  expect(announced()).toBe("Status: Ready");
  expect(host("mixer-sign").props.accessibilityLabel).toBe("on-air");
  expect(flat(textsIn(host("mixer-sign"))[0].props.style).color).toBe("#4a4136");

  ReactTestRenderer.act(() => tree?.unmount());
  api.getActiveOutput.mockResolvedValue({
    device_id: "uuid:mock-sonos",
    device_name: "Mock Sonos",
    transport: "sonos",
    state: "live",
  });
  await renderApp(); // the saved pairing restores straight into the mixer
  expect(announced()).toBe("Audio is live");
  expect(flat(textsIn(host("mixer-sign"))[0].props.style).color).toBe("#ff3b2a");
});

test("live state makes the lamp glow; ready does not", async () => {
  await renderApp();
  await pairWith();
  expect(flat(host("live-lamp").props.style).shadowOpacity).toBe(0);

  ReactTestRenderer.act(() => tree?.unmount());
  api.getActiveOutput.mockResolvedValue({
    device_id: "uuid:mock-sonos",
    device_name: "Mock Sonos",
    transport: "sonos",
    state: "live",
  });
  await renderApp();
  expect(flat(host("live-lamp").props.style)).toMatchObject({
    shadowColor: "#ff3b2a",
    shadowOpacity: 0.6,
  });
});

test("connection: refresh is a top-right toolbar button, disabled while it runs", async () => {
  await renderApp();
  await pairWith();
  await press("more-button");
  const refresh = toolbarButton("Refresh connection");
  expect(refresh.props.icon).toBe("arrow.clockwise");
  let toolbar = refresh.parent;
  while (toolbar && (toolbar.type as unknown) !== "NativeToolbar") toolbar = toolbar.parent;
  expect(toolbar?.props.placement).toBe("right");
  expect(refresh.props.disabled).toBe(false);
  // The old in-form button is gone; the footer still explains what refresh does.
  const formButtons = (tree as ReactTestRenderer.ReactTestRenderer).root.findAll(
    (node) => (node.type as unknown) === "ExpoButton" && node.props.label === "Refresh connection",
  );
  expect(formButtons).toHaveLength(0);
  expect(text()).toContain("Tap ↻ to reload without removing your pairing.");

  api.fetchStatus.mockReturnValue(new Promise(() => {})); // the refresh never answers
  ReactTestRenderer.act(() => {
    void toolbarButton("Refresh connection").props.onPress();
  });
  await flush();
  expect(toolbarButton("Refresh connection").props.disabled).toBe(true);
});

test("a firewall block keeps the live dot off, matching the header", async () => {
  api.getActiveOutput.mockResolvedValue({
    device_id: "uuid:mock-sonos",
    device_name: "Mock Sonos",
    transport: "sonos",
    state: "live",
  });
  api.fetchStatus.mockResolvedValue({
    status: "ok",
    version: "0.1.0",
    service_enabled: true,
    firewall_warning: {
      message: "UFW is blocking LAN audio.",
      subnet: "192.168.5.0/24",
      port: 47990,
      repair_command: "sudo ufw allow from 192.168.5.0/24 to any port 47990 proto tcp",
    },
  });
  await renderApp();
  await pairWith();
  expect(announced()).toBe("Firewall blocking audio");
  expect(host("led-status").props.children).toBe("BLOCKED");
  expect(flat(host("live-lamp").props.style).shadowOpacity).toBe(0);
  // The warning sits on the faceplate as plain text, not in a red box.
  const warning = flat(host("firewall-warning").props.style);
  expect(warning.backgroundColor).toBeUndefined();
  expect(warning.borderWidth).toBeUndefined();
});

test("the volume caption and printed scale cap text scaling", async () => {
  await renderApp();
  await pairWith();
  const printed = host("mixer-screen").findAll(
    (n) =>
      (n.type as unknown) === "Text" &&
      ["Volume", "0", "50", "100"].includes(n.props.children as string) &&
      n.props.accessibilityElementsHidden !== undefined,
  );
  const caption = host("mixer-screen").find(
    (n) => (n.type as unknown) === "Text" && n.props.children === "Volume",
  );
  expect(caption.props.maxFontSizeMultiplier).toBe(1.3);
  expect(printed.length).toBe(3); // the three scale marks
  for (const text of printed) expect(text.props.maxFontSizeMultiplier).toBe(1.3);
});

test("volume shows on an LED readout that dims when the desktop is paused", async () => {
  // Volume is only adjustable while a speaker is live.
  api.getActiveOutput.mockResolvedValue({
    device_id: "uuid:mock-sonos",
    device_name: "Mock Sonos",
    transport: "sonos",
    state: "live",
  });
  await renderApp();
  await pairWith();
  const value = () =>
    textsIn(host("volume-readout")).find((t) => !t.props.accessibilityElementsHidden);
  expect(host("volume-readout").props.accessibilityLabel).toBe("Volume 50");
  expect(value()?.props.children).toBe("50");
  expect(flat(value()?.props.style).color).toBe("#e0a24b");

  api.fetchStatus.mockResolvedValue({ status: "ok", version: "0.1.0", service_enabled: false });
  for (const read of [api.listInputs, api.listOutputs, api.getEq, api.getVolume]) {
    read.mockRejectedValue(
      new client.HttpError("/api/x", 503, '{"error":"","code":"service_paused"}'),
    );
  }
  await press("more-button");
  await pressRefresh();
  await press("native-back");
  expect(announced()).toBe("Desktop service is paused");
  expect(flat(value()?.props.style).color).toBe("#9a8f78");
});

test("pairing: unlit sign, LED code field and an amber Connect key", async () => {
  await renderApp();
  expect(host("remote-title").props.accessibilityLabel).toBe("on-air");
  expect(flat(textsIn(host("remote-title"))[0].props.style).color).toBe("#4a4136");
  expect(text()).toContain("Pair your Mac");

  const pin = host("pin-input");
  expect(pin.props).toMatchObject({
    placeholder: "888888",
    maxLength: 6,
    accessibilityLabel: "Six-digit pairing code",
    maxFontSizeMultiplier: 1.3,
  });
  expect(flat(pin.props.style)).toMatchObject({
    fontFamily: "DSEG14ClassicMini-Regular",
    color: "#e0a24b",
    backgroundColor: "#070605",
  });
  expect(pin.props.placeholderTextColor).toBe("#1c1812");

  expect(flat(host("pair-button").props.style)).toMatchObject({ backgroundColor: "#1d1a15" });
  ReactTestRenderer.act(() => host("pin-input").props.onChangeText("123456"));
  expect(flat(host("pair-button").props.style)).toMatchObject({ backgroundColor: "#e0a24b" });
  expect(flat(textsIn(host("pair-button"))[0].props.style).color).toBe("#141210");
});

test("pairing: text fields take an amber caret, not Android's default accent", async () => {
  await renderApp();
  await press("manual-setup-button");
  for (const id of ["host-input", "pin-input"]) {
    expect(host(id).props).toMatchObject({
      cursorColor: "#e0a24b",
      selectionHandleColor: "#e0a24b",
    });
  }
});

test("pairing is one faceplate: no bordered boxes", async () => {
  await renderApp();
  await press("manual-setup-button");
  const boxes = host("pairing-screen").findAll(
    (n) =>
      ["View", "Pressable", "TextInput"].includes(n.type as string) &&
      Number(flat(n.props.style).borderWidth ?? 0) > 0,
  );
  expect(boxes.map((b) => b.props.testID)).toEqual(["remote-title"]); // only the sign's housing
  expect(exists("faceplate-finish")).toBe(true);
});

test("pairing: the Connect key stays lit with a visible spinner while the code is checked", async () => {
  api.verifyPin.mockReturnValue(new Promise<string>(() => {})); // never answers
  await renderApp();
  ReactTestRenderer.act(() => host("pin-input").props.onChangeText("123456"));
  ReactTestRenderer.act(() => {
    void host("pair-button").props.onPress();
  });
  await flush();
  const key = host("pair-button");
  expect(key.props.onPress).toBeUndefined(); // busy: not pressable again
  expect(flat(key.props.style)).toMatchObject({ backgroundColor: "#e0a24b" });
  const spinner = key.find((n) => (n.type as unknown) === "ActivityIndicator");
  expect(spinner.props.color).toBe("#141210");
});
