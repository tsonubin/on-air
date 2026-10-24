import type { ReactElement } from "react";
import ReactTestRenderer, { type ReactTestInstance } from "react-test-renderer";
import { Faceplate, Groove } from "@/ui/faceplate";
import { isDsegText, LedReadout, LedText } from "@/ui/led-readout";
import { LedWindow } from "@/ui/led-window";
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
    color: "#4a4136",
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
  // The readout sits in the same recessed well as every LED window: a shadow, not a border.
  const well = flat(hostById(r, "led").props.style);
  expect(well.boxShadow).toBe("inset 0 2px 6px rgba(0,0,0,0.7), 0 1px 0 rgba(255,236,200,0.06)");
  expect(well.borderWidth).toBeUndefined();
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

test("faceplate: satin gradient, top highlight and a fine repeating grain, all hidden", () => {
  const r = create(<Faceplate testID="f">{null}</Faceplate>);
  const finish = hostById(r, "faceplate-finish");
  expect(finish.props).toMatchObject({
    pointerEvents: "none",
    accessibilityElementsHidden: true,
    importantForAccessibility: "no-hide-descendants",
  });
  expect(flat(finish.props.style).experimental_backgroundImage).toBe(
    "radial-gradient(ellipse 120% 45% at 50% 0%, rgba(255,236,200,0.07), transparent), linear-gradient(180deg, #1f1b16, #16130f)",
  );
  // The grain sits in one faint layer, tiled to cover the faceplate once it is measured.
  const grain = hostById(r, "faceplate-grain");
  expect(flat(grain.props.style).opacity).toBe(0.035);
  expect(grain.findAll((n) => isHost(n, "Image"))).toHaveLength(0);
});

test("faceplate grain tiles cover the whole plate, not just one corner", () => {
  // Found on device: resizeMode "repeat" drew a single 128 pt tile, a visible lighter square.
  const r = create(<Faceplate testID="f">{null}</Faceplate>);
  ReactTestRenderer.act(() =>
    hostById(r, "faceplate-finish").props.onLayout({
      nativeEvent: { layout: { x: 0, y: 0, width: 390, height: 844 } },
    }),
  );
  const tiles = hostById(r, "faceplate-grain").findAll((n) => isHost(n, "Image"));
  expect(tiles).toHaveLength(4 * 7); // ceil(390 / 128) × ceil(844 / 128)
  expect(flat(tiles[0].props.style)).toMatchObject({ width: 128, height: 128, left: 0, top: 0 });
  expect(flat(tiles[tiles.length - 1].props.style)).toMatchObject({ left: 384, top: 768 });
});

test("groove: an engraved dark line over a faint light one", () => {
  const r = create(<Groove testID="g" />);
  expect(hostById(r, "g").props.accessibilityElementsHidden).toBe(true);
  expect(flat(hostById(r, "g").props.style)).toMatchObject({
    borderTopWidth: 1,
    borderTopColor: "#0d0b09",
    borderBottomWidth: 1,
    borderBottomColor: "rgba(255,236,200,0.06)",
  });
});

test("LED text: ASCII in 14-segment amber, uppercased by style, one line", () => {
  const r = create(
    <LedText size={18} testID="t">
      Living room
    </LedText>,
  );
  const t = hostById(r, "t");
  expect(t.props.children).toBe("Living room");
  expect(t.props).toMatchObject({
    numberOfLines: 1,
    ellipsizeMode: "tail",
    adjustsFontSizeToFit: true,
    minimumFontScale: 0.75,
    maxFontSizeMultiplier: 1.3,
  });
  expect(flat(t.props.style)).toMatchObject({
    fontFamily: "DSEG14ClassicMini-Regular",
    fontSize: 18,
    textTransform: "uppercase",
    color: "#e0a24b",
  });
});

test("LED text: names DSEG14 cannot draw fall back to Plex Mono and still stay on one line", () => {
  const name = "Küche – HomePod 🎵 Wohnzimmer Lautsprecher";
  const t = hostById(
    create(
      <LedText size={18} testID="t">
        {name}
      </LedText>,
    ),
    "t",
  );
  expect(flat(t.props.style).fontFamily).toBe("IBMPlexMono-Medium");
  expect(t.props.numberOfLines).toBe(1);
  expect(isDsegText("ON AIR 44.1K")).toBe(true);
  expect(isDsegText("Café")).toBe(false);
});

test("LED text: dim is muted with no glow", () => {
  const t = hostById(
    create(
      <LedText size={18} dim testID="t">
        Choose a source
      </LedText>,
    ),
    "t",
  );
  expect(flat(t.props.style)).toMatchObject({ color: "#9a8f78", textShadowRadius: 0 });
});

test("LED window: a recessed well with no border", () => {
  const r = create(<LedWindow testID="w" />);
  const style = flat(hostById(r, "w").props.style);
  expect(style).toMatchObject({
    backgroundColor: "#070605",
    borderRadius: 6,
    boxShadow: "inset 0 2px 6px rgba(0,0,0,0.7), 0 1px 0 rgba(255,236,200,0.06)",
    paddingVertical: 12,
    paddingHorizontal: 14,
  });
  expect(style.borderWidth).toBeUndefined();
});
