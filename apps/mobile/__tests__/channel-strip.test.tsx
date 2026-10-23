import type { ReactElement } from "react";
import ReactTestRenderer, { type ReactTestInstance } from "react-test-renderer";
import { ChannelStrip } from "@/ui/channel-strip";
import { Lamp, type LampState } from "@/ui/lamp";

// Style props are arbitrarily nested arrays of objects; flatten like StyleSheet.flatten.
const flat = (style: any): Record<string, unknown> =>
  Array.isArray(style) ? Object.assign({}, ...style.map(flat)) : (style ?? {});

function create(element: ReactElement) {
  let renderer!: ReactTestRenderer.ReactTestRenderer;
  ReactTestRenderer.act(() => {
    renderer = ReactTestRenderer.create(element);
  });
  return renderer;
}

// The react-native mock renders host elements named by string ("Text", "View").
const isHost = (n: ReactTestInstance, name: string) => (n.type as unknown) === name;
const hostById = (r: ReactTestRenderer.ReactTestRenderer, id: string) =>
  r.root.find((n) => typeof n.type === "string" && n.props.testID === id);
const textWith = (r: ReactTestRenderer.ReactTestRenderer, children: string) =>
  r.root.find((n) => isHost(n, "Text") && n.props.children === children);

test("lamps glow only when amber or live", () => {
  const style = (state: LampState) =>
    flat(hostById(create(<Lamp state={state} testID="l" />), "l").props.style);
  expect(style("off")).toMatchObject({
    width: 7,
    height: 7,
    borderRadius: 3.5,
    backgroundColor: "#3a332a",
    shadowOpacity: 0,
  });
  expect(style("amber")).toMatchObject({
    backgroundColor: "#e0a24b",
    shadowColor: "#e0a24b",
    shadowOpacity: 0.6,
  });
  expect(style("live")).toMatchObject({
    backgroundColor: "#ff3b2a",
    shadowColor: "#ff3b2a",
    shadowOpacity: 0.6,
  });
  expect(style("error")).toMatchObject({ backgroundColor: "#f0b4ac", shadowOpacity: 0 });
});

test.each([
  ["idle", "#3a332a", "rgba(42,38,31,0.55)", "#2e2a24", "#e0a24b"],
  ["selected", "#e0a24b", "rgba(42,38,31,0.55)", "#5c4a32", "#f0c56a"],
  ["connecting", "#e0a24b", "rgba(42,38,31,0.55)", "#2e2a24", "#e0a24b"],
  ["live", "#ff3b2a", "#3a221c", "#8a3a30", "#ff3b2a"],
  ["failed", "#f0b4ac", "#3a1410", "#6a2a22", "#f0b4ac"],
  ["unavailable", "#3a332a", "rgba(42,38,31,0.55)", "#2e2a24", "#9a8f78"],
] as const)("a %s strip", (tone, lamp, surface, border, ink) => {
  const r = create(
    <ChannelStrip
      name="Den"
      caption="Sonos"
      action="Go"
      tone={tone}
      onPress={jest.fn()}
      testID="s"
    />,
  );
  expect(flat(hostById(r, "s-lamp").props.style).backgroundColor).toBe(lamp);
  expect(flat(hostById(r, "s").props.style)).toMatchObject({
    backgroundColor: surface,
    borderColor: border,
    minHeight: 44,
  });
  expect(flat(hostById(r, "s-action").props.style).color).toBe(ink);
  expect(flat(hostById(r, "s").props.style).opacity).toBe(
    tone === "unavailable" ? 0.45 : undefined,
  );
});

test("a strip reads as one button with its state", () => {
  const r = create(
    <ChannelStrip
      name="Den"
      caption="Sonos"
      action="Live"
      tone="live"
      onPress={jest.fn()}
      testID="s"
    />,
  );
  expect(hostById(r, "s").props).toMatchObject({
    accessibilityRole: "button",
    accessibilityLabel: "Den, Sonos",
    accessibilityValue: { text: "Live" },
    accessibilityState: { selected: true, busy: false, disabled: false },
  });
  const busy = create(
    <ChannelStrip
      name="Den"
      caption="Sonos"
      action="Connecting…"
      tone="connecting"
      onPress={jest.fn()}
      testID="s"
    />,
  );
  expect(hostById(busy, "s").props.accessibilityState).toMatchObject({
    selected: false,
    busy: true,
  });
});

test("a disabled strip ignores presses", () => {
  const r = create(
    <ChannelStrip
      name="Den"
      caption="Sonos"
      action="Connect"
      tone="idle"
      disabled
      onPress={jest.fn()}
      testID="s"
    />,
  );
  expect(hostById(r, "s").props.onPress).toBeUndefined();
  expect(hostById(r, "s").props.accessibilityState).toMatchObject({ disabled: true });
});

test("a long name wraps beside a whole action word", () => {
  const r = create(
    <ChannelStrip
      name="Living Room Sonos Arc Ultra with Sub Mini"
      caption="Sonos"
      action="Failed · Retry"
      tone="failed"
      onPress={jest.fn()}
      testID="s"
    />,
  );
  const name = textWith(r, "Living Room Sonos Arc Ultra with Sub Mini");
  expect(name.props).toMatchObject({ numberOfLines: 2, maxFontSizeMultiplier: 1.3 });
  expect(flat(hostById(r, "s-text").props.style).flex).toBe(1);
  const action = hostById(r, "s-action");
  expect(action.props).toMatchObject({ numberOfLines: 1, maxFontSizeMultiplier: 1.3 });
  expect(flat(action.props.style).flexShrink).toBe(0);
});
