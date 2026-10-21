import type { ReactElement } from "react";
import ReactTestRenderer, { type ReactTestInstance } from "react-test-renderer";
import { Card } from "@/ui/card";
import { IconTile } from "@/ui/icon-tile";
import { LedReadout } from "@/ui/led-readout";
import { Sign } from "@/ui/sign";

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
const texts = (node: ReactTestInstance) => node.findAll((n) => isHost(n, "Text"));
const textWith = (r: ReactTestRenderer.ReactTestRenderer, children: string) =>
  r.root.find((n) => isHost(n, "Text") && n.props.children === children);

test("the sign is named on-air and lights only when live", () => {
  const lit = create(<Sign lit testID="s" />);
  expect(hostById(lit, "s").props).toMatchObject({
    accessible: true,
    accessibilityRole: "header",
    accessibilityLabel: "on-air",
  });
  expect(flat(textWith(lit, "ONAIR").props.style)).toMatchObject({
    color: "#ff3b2a",
    fontFamily: "Anton-Regular",
    fontSize: 18,
  });
  expect(flat(hostById(lit, "s").props.style)).toMatchObject({
    backgroundColor: "#1a0605",
    borderColor: "#8a3a30",
  });

  const unlit = create(<Sign lit={false} testID="s" />);
  expect(flat(textWith(unlit, "ONAIR").props.style)).toMatchObject({
    color: "#5a1814",
    textShadowRadius: 0,
  });
  expect(flat(hostById(unlit, "s").props.style)).toMatchObject({ backgroundColor: "#0e0c0a" });
});

test("the sign and LED digits cap text scaling", () => {
  const sign = create(<Sign lit />);
  const led = create(<LedReadout value="70" digits={3} size={20} accessibilityLabel="Volume 70" />);
  for (const t of [...texts(sign.root), ...texts(led.root)]) {
    expect(t.props.maxFontSizeMultiplier).toBe(1.3);
  }
});

test("LED readout: ghost segments are hidden, the value is amber, dim is muted", () => {
  const r = create(
    <LedReadout value="70" digits={3} size={20} accessibilityLabel="Volume 70" testID="led" />,
  );
  expect(hostById(r, "led").props).toMatchObject({
    accessible: true,
    accessibilityLabel: "Volume 70",
  });
  expect(textWith(r, "888").props).toMatchObject({
    accessibilityElementsHidden: true,
    importantForAccessibility: "no",
  });
  expect(flat(textWith(r, "70").props.style)).toMatchObject({
    color: "#e0a24b",
    fontFamily: "DSEG14ClassicMini-Regular",
    fontSize: 20,
  });

  const dim = create(
    <LedReadout value="70" digits={3} size={20} dim accessibilityLabel="Volume 70" />,
  );
  expect(flat(textWith(dim, "70").props.style)).toMatchObject({
    color: "#9a8f78",
    textShadowRadius: 0,
  });
});

test("faceplate card: machined radius and a lighter top edge", () => {
  const r = create(<Card testID="c">{null}</Card>);
  expect(flat(hostById(r, "c").props.style)).toMatchObject({
    backgroundColor: "#1a1814",
    borderRadius: 8,
    borderColor: "#3a342a",
    borderTopColor: "#4a4338",
  });
});

test("icon tiles use the machined medium radius", () => {
  const r = create(<IconTile name="speaker" />);
  const tile = r.root.find((n) => isHost(n, "View") && flat(n.props.style).width === 54);
  expect(flat(tile.props.style)).toMatchObject({ borderRadius: 6, backgroundColor: "#2a261f" });
});
