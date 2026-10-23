import type { ReactElement } from "react";
import { Platform, Text } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import ReactTestRenderer, { type ReactTestInstance } from "react-test-renderer";
import type { DevicePairTarget } from "@/remote-session";
import { DevicePairSheet } from "@/sheets/device-pair-sheet";
import { OutputSheet } from "@/sheets/output-sheet";
import { SheetPanel } from "@/sheets/sheet-panel";
import { SourceSheet } from "@/sheets/source-sheet";

let mockSession: Record<string, unknown>;
jest.mock("@/remote-session", () => ({
  ...jest.requireActual("@/remote-session"),
  useRemoteSession: () => mockSession,
}));

// Style props are arbitrarily nested arrays of objects; flatten like StyleSheet.flatten.
const flat = (style: any): Record<string, unknown> =>
  Array.isArray(style) ? Object.assign({}, ...style.map(flat)) : (style ?? {});

/** All the text a tree renders, joined, for copy checks. */
const textOf = (r: ReactTestRenderer.ReactTestRenderer) =>
  r.root
    .findAll((n) => (n.type as unknown) === "Text")
    .map((n) => [n.props.children].flat().join(""))
    .join(" ");

/** Whether a node's subtree holds a host element with this testID, or a text containing this copy. */
const holds = (node: ReactTestInstance, { id, text }: { id?: string; text?: string }) =>
  node.findAll(
    (n) =>
      (id !== undefined && typeof n.type === "string" && n.props.testID === id) ||
      (text !== undefined &&
        (n.type as unknown) === "Text" &&
        [n.props.children].flat().join("") === text),
  ).length > 0;

async function press(node: ReactTestInstance) {
  const handler = node.props.onPress;
  if (!handler) throw new Error(`"${node.props.testID}" is disabled`);
  await ReactTestRenderer.act(async () => {
    await handler();
  });
}

function create(element: ReactElement) {
  let renderer!: ReactTestRenderer.ReactTestRenderer;
  ReactTestRenderer.act(() => {
    renderer = ReactTestRenderer.create(element);
  });
  return renderer;
}

const hostById = (r: ReactTestRenderer.ReactTestRenderer, id: string): ReactTestInstance =>
  r.root.find((n) => typeof n.type === "string" && n.props.testID === id);

test("a sheet panel is a full-bleed faceplate with a caption and Done", () => {
  const onClose = jest.fn();
  const r = create(
    <SheetPanel spec={{ kind: "source", count: 1 }} caption="Source" onClose={onClose} testID="p">
      <Text>Body</Text>
    </SheetPanel>,
  );
  const sheet = r.root.find((n) => (n.type as unknown) === "ExpoBottomSheet");
  expect(sheet.props).toMatchObject({
    isPresented: true,
    contentPadding: 0,
    containerColor: "#16130f",
    testID: "p",
  });
  // The native sheet already keeps content clear of the home indicator; no second inset.
  // The body fills the sheet so a sheet's scroll view is bounded by its detent.
  expect(flat(hostById(r, "p-body").props.style)).toMatchObject({ paddingBottom: 16, flex: 1 });
  sheet.props.onDismiss(); // swipe-down / Android back
  expect(onClose).toHaveBeenCalledTimes(1);
  expect(hostById(r, "p-caption").props).toMatchObject({
    accessibilityRole: "header",
    children: "Source",
  });
  hostById(r, "p-done").props.onPress();
  expect(onClose).toHaveBeenCalledTimes(2);
  expect(
    r.root.findAll((n) => n.props.testID === "faceplate-finish" && typeof n.type === "string"),
  ).toHaveLength(1);
});

test("the faceplate fills an iOS detent and grows to its content on Android", () => {
  const host = () =>
    create(
      <SheetPanel
        spec={{ kind: "source", count: 1 }}
        caption="Source"
        onClose={jest.fn()}
        testID="p"
      >
        <Text>Body</Text>
      </SheetPanel>,
    ).root.find((n) => (n.type as unknown) === "ExpoRNHostView");
  const original = Platform.OS;
  try {
    (Platform as { OS: string }).OS = "ios";
    expect(host().props.matchContents).toBe(false);
    (Platform as { OS: string }).OS = "android";
    expect(host().props.matchContents).toEqual({ vertical: true });
  } finally {
    (Platform as { OS: string }).OS = original;
  }
});

test("beside a side rail the plate runs under it and the content keeps clear", () => {
  jest.mocked(useSafeAreaInsets).mockReturnValue({ top: 0, right: 84, bottom: 34, left: 0 });
  try {
    const r = create(
      <SheetPanel
        spec={{ kind: "source", count: 1 }}
        caption="Source"
        onClose={jest.fn()}
        testID="p"
      >
        <Text>Body</Text>
      </SheetPanel>,
    );
    expect(flat(hostById(r, "p-body").props.style)).toMatchObject({
      paddingLeft: 16,
      paddingRight: 84,
    });
  } finally {
    jest.mocked(useSafeAreaInsets).mockReturnValue({ top: 47, right: 0, bottom: 34, left: 0 });
  }
});

test("the pair panel has no Done key", () => {
  const r = create(
    <SheetPanel
      spec={{ kind: "device-pair", transport: "airplay" }}
      caption="Pair speaker"
      done={false}
      onClose={jest.fn()}
      testID="p"
    >
      <Text>Body</Text>
    </SheetPanel>,
  );
  expect(r.root.findAll((n) => n.props.testID === "p-done")).toHaveLength(0);
});

describe("source sheet", () => {
  const session = (overrides: Record<string, unknown> = {}) => ({
    inputs: [
      { name: "mock.monitor", label: "Mock Monitor" },
      { name: "mic", label: "MacBook Pro Microphone" },
    ],
    activeInput: "mock.monitor",
    activeInputLabel: "Mock Monitor",
    busyTarget: null,
    view: { controlsEnabled: true },
    pickInput: jest.fn(async () => true),
    ...overrides,
  });

  test("the LED window shows the chosen source, or a dim No source", () => {
    mockSession = session();
    expect(hostById(create(<SourceSheet onClose={jest.fn()} />), "source-led").props.children).toBe(
      "Mock Monitor",
    );
    mockSession = session({ activeInput: null, activeInputLabel: "" });
    const led = hostById(create(<SourceSheet onClose={jest.fn()} />), "source-led");
    expect(led.props.children).toBe("No source");
    expect(flat(led.props.style).color).toBe("#9a8f78");
  });

  test("each capture device is a strip; the chosen one is selected", () => {
    mockSession = session();
    const r = create(<SourceSheet onClose={jest.fn()} />);
    expect(hostById(r, "input-mock.monitor").props).toMatchObject({
      accessibilityLabel: "Mock Monitor, Capture device",
      accessibilityValue: { text: "Selected" },
      accessibilityState: { selected: true },
    });
    expect(hostById(r, "input-mic").props).toMatchObject({
      accessibilityValue: { text: "Select" },
      accessibilityState: { selected: false, busy: false },
    });
  });

  test("picking a device switches source and closes only on success", async () => {
    const onClose = jest.fn();
    mockSession = session();
    await press(hostById(create(<SourceSheet onClose={onClose} />), "input-mic"));
    expect(mockSession.pickInput).toHaveBeenCalledWith("mic");
    expect(onClose).toHaveBeenCalledTimes(1);

    const stays = jest.fn();
    mockSession = session({ pickInput: jest.fn(async () => false) });
    await press(hostById(create(<SourceSheet onClose={stays} />), "input-mic"));
    expect(stays).not.toHaveBeenCalled();
  });

  test("tapping the chosen device just closes the sheet", async () => {
    const onClose = jest.fn();
    mockSession = session();
    await press(hostById(create(<SourceSheet onClose={onClose} />), "input-mock.monitor"));
    expect(mockSession.pickInput).not.toHaveBeenCalled();
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  test("while a switch runs, its strip reads Switching… and no strip takes a tap", () => {
    mockSession = session({ busyTarget: "input:mic" });
    const r = create(<SourceSheet onClose={jest.fn()} />);
    expect(hostById(r, "input-mic").props).toMatchObject({
      accessibilityValue: { text: "Switching…" },
      accessibilityState: { busy: true },
    });
    expect(hostById(r, "input-mic").props.onPress).toBeUndefined();
    expect(hostById(r, "input-mock.monitor").props.onPress).toBeUndefined();
  });

  test("with controls off the strips disable but Done still closes", async () => {
    const onClose = jest.fn();
    mockSession = session({ view: { controlsEnabled: false } });
    const r = create(<SourceSheet onClose={onClose} />);
    expect(hostById(r, "input-mic").props.onPress).toBeUndefined();
    expect(hostById(r, "input-mock.monitor").props.onPress).toBeUndefined();
    await press(hostById(r, "source-sheet-done"));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  test("at large text sizes the footnote scrolls with the strips", () => {
    mockSession = session();
    const scroll = hostById(create(<SourceSheet onClose={jest.fn()} />), "source-sheet-scroll");
    expect(flat(scroll.props.style).flexShrink).toBe(1);
    expect(
      holds(scroll, { text: "The Mac captures this device and streams it to the speaker." }),
    ).toBe(true);
  });

  test("the footnote explains the pick, or says none are available", () => {
    mockSession = session();
    expect(textOf(create(<SourceSheet onClose={jest.fn()} />))).toContain(
      "The Mac captures this device and streams it to the speaker.",
    );
    mockSession = session({ inputs: [], activeInput: null, activeInputLabel: "" });
    expect(textOf(create(<SourceSheet onClose={jest.fn()} />))).toContain(
      "No capture devices are available.",
    );
  });
});

describe("speaker sheet", () => {
  const speaker = (transport: string, id: string, name: string, extra = {}) => ({
    transport,
    id,
    name,
    kind: "solo",
    member_count: 1,
    needs_pair: false,
    paired: true,
    ...extra,
  });
  const SONOS = speaker("sonos", "uuid:mock-sonos", "Mock Sonos");
  const AIRPLAY = speaker("airplay", "ap-living", "Living Room AirPlay");
  const BT = speaker("bluetooth", "bt-mock", "Mock Bluetooth Speaker");
  const NEW_BT = speaker("bluetooth", "bt-new", "New Bluetooth Speaker", {
    needs_pair: true,
    paired: false,
  });
  const session = (overrides: Record<string, unknown> = {}) => ({
    outputs: [SONOS, AIRPLAY, BT, NEW_BT],
    activeOutput: {
      transport: "sonos",
      device_id: "uuid:mock-sonos",
      device_name: "Mock Sonos",
      state: "live",
    },
    airplayMode: "avroute-picker",
    busyTarget: null,
    view: { controlsEnabled: true },
    activate: jest.fn(async () => true),
    openBluetoothSettings: jest.fn(async () => {}),
    ...overrides,
  });
  const sheet = (onClose = jest.fn(), onPairRequest = jest.fn()) =>
    create(<OutputSheet onClose={onClose} onPairRequest={onPairRequest} />);
  const strips = [
    "sonos-uuid:mock-sonos",
    "airplay-ap-living",
    "bluetooth-bt-mock",
    "bluetooth-bt-new",
  ];

  test("the playing speaker's strip is live and the LED window says so", () => {
    mockSession = session();
    const r = sheet();
    expect(hostById(r, "output-sonos-uuid:mock-sonos-action").props.children).toBe("Connected");
    expect(flat(hostById(r, "output-sonos-uuid:mock-sonos-lamp").props.style).backgroundColor).toBe(
      "#ff3b2a",
    );
    expect(flat(hostById(r, "output-sonos-uuid:mock-sonos").props.style).backgroundColor).toBe(
      "#3a221c",
    );
    expect(hostById(r, "speaker-led").props.children).toBe("Mock Sonos");
    expect(hostById(r, "speaker-led-state").props.children).toBe("Live");
  });

  test("desktop-only AirPlay is unavailable, with a footnote saying where to pick it", () => {
    mockSession = session();
    const r = sheet();
    const row = hostById(r, "output-airplay-ap-living");
    expect(row.props.accessibilityValue).toEqual({ text: "Desktop only" });
    expect(row.props.onPress).toBeUndefined();
    expect(textOf(r)).toContain("AirPlay speakers are picked from the desktop.");
  });

  test("a starting speaker reads Connecting…; a failed one offers a retry", () => {
    mockSession = session({
      activeOutput: { ...(session().activeOutput as object), state: "starting" },
    });
    expect(hostById(sheet(), "speaker-led-state").props.children).toBe("Connecting…");
    mockSession = session({
      activeOutput: { ...(session().activeOutput as object), state: "failed" },
    });
    const row = hostById(sheet(), "output-sonos-uuid:mock-sonos");
    expect(row.props.accessibilityValue).toEqual({ text: "Failed · Retry" });
    expect(flat(row.props.style).backgroundColor).toBe("#3a1410");
  });

  test("an unpaired speaker asks to pair instead of connecting", async () => {
    const onPairRequest = jest.fn();
    mockSession = session();
    await press(hostById(sheet(jest.fn(), onPairRequest), "output-bluetooth-bt-new"));
    expect(onPairRequest).toHaveBeenCalledWith(NEW_BT);
    expect(mockSession.activate).not.toHaveBeenCalled();
  });

  test("connecting a speaker closes the sheet on success", async () => {
    const onClose = jest.fn();
    mockSession = session();
    await press(hostById(sheet(onClose), "output-bluetooth-bt-mock"));
    expect(mockSession.activate).toHaveBeenCalledWith(BT);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  test("Add Bluetooth speaker opens the desktop's Bluetooth settings", async () => {
    mockSession = session();
    await press(hostById(sheet(), "add-bluetooth"));
    expect(mockSession.openBluetoothSettings).toHaveBeenCalledTimes(1);
  });

  test("while a speaker connects, no strip takes a tap", () => {
    mockSession = session({ busyTarget: "bluetooth:bt-mock" });
    const r = sheet();
    for (const id of strips) expect(hostById(r, `output-${id}`).props.onPress).toBeUndefined();
  });

  test("with controls off the strips and Add disable but Done still closes", async () => {
    const onClose = jest.fn();
    mockSession = session({ view: { controlsEnabled: false } });
    const r = sheet(onClose);
    for (const id of strips) expect(hostById(r, `output-${id}`).props.onPress).toBeUndefined();
    expect(hostById(r, "add-bluetooth").props.onPress).toBeUndefined();
    await press(hostById(r, "output-sheet-done"));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  test("with no speakers yet the window reads Scanning", () => {
    mockSession = session({ outputs: [], activeOutput: null });
    const r = sheet();
    expect(hostById(r, "speaker-led").props.children).toBe("Scanning");
    expect(textOf(r)).toContain("Waiting for speakers on the LAN…");
  });

  test("at large text sizes the Add key and footnotes scroll with the strips", () => {
    mockSession = session();
    const scroll = hostById(sheet(), "output-sheet-scroll");
    expect(flat(scroll.props.style).flexShrink).toBe(1);
    expect(holds(scroll, { id: "add-bluetooth" })).toBe(true);
    expect(holds(scroll, { text: "AirPlay speakers are picked from the desktop." })).toBe(true);
  });

  test("the window keeps the active speaker even when it drops off the list", () => {
    mockSession = session({ outputs: [BT, NEW_BT] });
    expect(hostById(sheet(), "speaker-led").props.children).toBe("Mock Sonos");
  });
});

describe("pair speaker sheet", () => {
  const AIRPLAY: DevicePairTarget = { transport: "airplay", id: "ap", name: "Living Room AirPlay" };
  const BLUETOOTH: DevicePairTarget = {
    transport: "bluetooth",
    id: "bt",
    name: "New Bluetooth Speaker",
  };
  const session = (overrides: Record<string, unknown> = {}) => ({
    devicePairing: false,
    pairDevice: jest.fn(async () => true),
    ...overrides,
  });

  test("AirPlay asks for the speaker's code in an LED field", () => {
    mockSession = session();
    const r = create(<DevicePairSheet target={AIRPLAY} onClose={jest.fn()} />);
    expect(hostById(r, "pair-sheet-caption").props.children).toBe("Pair speaker");
    expect(textOf(r)).toContain("Living Room AirPlay");
    expect(textOf(r)).toContain("Enter the code the speaker shows. No code? Leave it blank.");
    expect(hostById(r, "device-pin-input").props.maxLength).toBe(8);
    expect(r.root.findAll((n) => n.props.testID === "pair-sheet-done")).toHaveLength(0);
  });

  test("the code keeps digits only, up to eight, and pairs with them", async () => {
    const onClose = jest.fn();
    mockSession = session();
    const r = create(<DevicePairSheet target={AIRPLAY} onClose={onClose} />);
    ReactTestRenderer.act(() => hostById(r, "device-pin-input").props.onChangeText("12a34b5678x9"));
    expect(hostById(r, "device-pin-input").props.value).toBe("12345678");
    await press(hostById(r, "device-pair-submit"));
    expect(mockSession.pairDevice).toHaveBeenCalledWith(AIRPLAY, "12345678");
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  test("Cancel closes without pairing", async () => {
    const onClose = jest.fn();
    mockSession = session();
    await press(
      hostById(
        create(<DevicePairSheet target={AIRPLAY} onClose={onClose} />),
        "device-pair-cancel",
      ),
    );
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(mockSession.pairDevice).not.toHaveBeenCalled();
  });

  test("while pairing, the key reads Pairing… and neither key takes a tap", () => {
    mockSession = session({ devicePairing: true });
    const r = create(<DevicePairSheet target={AIRPLAY} onClose={jest.fn()} />);
    expect(hostById(r, "device-pair-submit").props.accessibilityLabel).toBe("Pairing…");
    expect(hostById(r, "device-pair-submit").props.onPress).toBeUndefined();
    expect(hostById(r, "device-pair-cancel").props.onPress).toBeUndefined();
  });

  test("Bluetooth confirms on the speaker, with no code field", () => {
    mockSession = session();
    const r = create(<DevicePairSheet target={BLUETOOTH} onClose={jest.fn()} />);
    expect(r.root.findAll((n) => n.props.testID === "device-pin-input")).toHaveLength(0);
    expect(textOf(r)).toContain(
      "Confirm pairing on the speaker or in the computer’s Bluetooth settings, then continue.",
    );
  });
});
