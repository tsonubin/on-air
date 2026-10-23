import type { ReactElement } from "react";
import { Text } from "react-native";
import ReactTestRenderer, { type ReactTestInstance } from "react-test-renderer";
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
    containerColor: "#1f1b16",
    testID: "p",
  });
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
