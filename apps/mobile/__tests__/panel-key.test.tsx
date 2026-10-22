import type { ReactElement } from "react";
import ReactTestRenderer, { type ReactTestInstance } from "react-test-renderer";
import { keyFace, PanelKey } from "@/ui/panel-key";

// Style props are arbitrarily nested arrays of objects; flatten like StyleSheet.flatten.
const flat = (style: any): Record<string, unknown> =>
  Array.isArray(style) ? Object.assign({}, ...style.filter(Boolean).map(flat)) : (style ?? {});

function create(element: ReactElement) {
  let renderer!: ReactTestRenderer.ReactTestRenderer;
  ReactTestRenderer.act(() => {
    renderer = ReactTestRenderer.create(element);
  });
  return renderer;
}

const isHost = (n: ReactTestInstance, name: string) => (n.type as unknown) === name;
const hostById = (r: ReactTestRenderer.ReactTestRenderer, id: string) =>
  r.root.find((n) => typeof n.type === "string" && n.props.testID === id);
const exists = (r: ReactTestRenderer.ReactTestRenderer, id: string) =>
  r.root.findAll((n) => typeof n.type === "string" && n.props.testID === id).length > 0;

test("key face: beveled gradient that inverts when pressed; lit keys are amber", () => {
  expect(keyFace({ pressed: false, lit: false })).toEqual({
    backgroundColor: "#1d1a15",
    experimental_backgroundImage: "linear-gradient(180deg, #2a261f, #1d1a15)",
    boxShadow: "inset 0 1px 0 rgba(255,236,200,0.08), 0 2px 0 #0a0908",
    transform: [{ translateY: 0 }],
  });
  expect(keyFace({ pressed: true, lit: false })).toEqual({
    backgroundColor: "#1d1a15",
    experimental_backgroundImage: "linear-gradient(180deg, #1d1a15, #2a261f)",
    boxShadow: "inset 0 1px 2px rgba(0,0,0,0.5)",
    transform: [{ translateY: 1 }],
  });
  expect(keyFace({ pressed: false, lit: true })).toEqual({
    backgroundColor: "#e0a24b",
    boxShadow: "inset 0 1px 0 rgba(255,255,255,0.25), 0 2px 0 #0a0908",
    transform: [{ translateY: 0 }],
  });
});

test("a key is a labelled button with a capped mono caption", () => {
  const r = create(<PanelKey label="Source" onPress={jest.fn()} testID="k" />);
  const key = hostById(r, "k");
  expect(key.props).toMatchObject({ accessibilityRole: "button", accessibilityLabel: "Source" });
  expect(flat(key.props.style)).toMatchObject({ minHeight: 56, borderRadius: 4 });
  const caption = key.find((n) => isHost(n, "Text"));
  expect(caption.props.children).toBe("Source");
  expect(caption.props.maxFontSizeMultiplier).toBe(1.3);
  expect(flat(caption.props.style)).toMatchObject({
    fontFamily: "IBMPlexMono-Medium",
    color: "#f3ead4",
  });
});

test("a disabled key can't be pressed and dims", () => {
  const r = create(<PanelKey label="EQ" onPress={jest.fn()} disabled testID="k" />);
  const key = hostById(r, "k");
  expect(key.props.onPress).toBeUndefined();
  expect(key.props.accessibilityState).toEqual({ disabled: true, busy: false });
  expect(flat(key.props.style).opacity).toBe(0.45);
});

test("the indicator is an amber dot", () => {
  const r = create(<PanelKey label="EQ" onPress={jest.fn()} indicator testID="k" />);
  expect(flat(hostById(r, "k-indicator").props.style)).toMatchObject({
    width: 6,
    height: 6,
    backgroundColor: "#e0a24b",
    // Pinned above the caption, so a key with a dot keeps its caption in line with the others.
    position: "absolute",
    top: 10,
  });
  expect(
    exists(create(<PanelKey label="EQ" onPress={jest.fn()} testID="k" />), "k-indicator"),
  ).toBe(false);
});

test("a lit busy key shows a dark spinner instead of its caption", () => {
  const r = create(<PanelKey label="Connect" onPress={jest.fn()} lit busy testID="k" />);
  const key = hostById(r, "k");
  expect(key.find((n) => isHost(n, "ActivityIndicator")).props.color).toBe("#141210");
  expect(key.findAll((n) => isHost(n, "Text"))).toHaveLength(0);
  expect(key.props.accessibilityState.busy).toBe(true);
});

test("a busy key can't be pressed but stays at full strength so its spinner shows", () => {
  const r = create(<PanelKey label="Connect" onPress={jest.fn()} lit busy disabled testID="k" />);
  const key = hostById(r, "k");
  expect(key.props.onPress).toBeUndefined();
  expect(flat(key.props.style).opacity).toBeUndefined();
});

test("a round key is a 36 pt icon button with hit slop to 44", () => {
  const r = create(
    <PanelKey
      shape="round"
      icon="ellipsis"
      accessibilityLabel="More options"
      onPress={jest.fn()}
      testID="k"
    />,
  );
  const key = hostById(r, "k");
  expect(key.props.accessibilityLabel).toBe("More options");
  expect(key.props.hitSlop).toBe(4);
  expect(flat(key.props.style)).toMatchObject({ width: 36, height: 36, borderRadius: 18 });
});
