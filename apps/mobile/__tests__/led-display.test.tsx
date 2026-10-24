import type { ReactElement } from "react";
import ReactTestRenderer from "react-test-renderer";
import { LedDisplay } from "@/led-display";
import type { HomeStatus } from "@/ui-state";

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

const hostById = (r: ReactTestRenderer.ReactTestRenderer, id: string) =>
  r.root.find((n) => typeof n.type === "string" && n.props.testID === id);

const LIVE: HomeStatus = { word: "ON AIR", announcement: "Audio is live", live: true };
const STANDBY: HomeStatus = { word: "STANDBY", announcement: "Status: Ready", live: false };
const RATE = { text: "44.1K", spoken: "44.1 kilohertz stereo" };

test("the window shows source, speaker, status and format, and reads as one summary", () => {
  const r = create(<LedDisplay source="Spotify" speaker="Living room" status={LIVE} rate={RATE} />);
  expect(hostById(r, "led-window").props).toMatchObject({
    accessible: true,
    accessibilityRole: "text",
    accessibilityLabel:
      "Audio is live. Source Spotify. Speaker Living room. 44.1 kilohertz stereo.",
  });
  expect(hostById(r, "led-source").props.children).toBe("Spotify");
  expect(hostById(r, "led-speaker").props.children).toBe("Living room");
  expect(hostById(r, "led-status").props.children).toBe("ON AIR");
  expect(hostById(r, "led-format").props.children).toBe("44.1K · ST");
  expect(flat(hostById(r, "live-lamp").props.style)).toMatchObject({
    backgroundColor: "#ff3b2a",
    shadowColor: "#ff3b2a",
    shadowOpacity: 0.6,
  });
});

test("the window's printed captions cap text scaling like the LEDs", () => {
  const r = create(<LedDisplay source="Spotify" speaker="Den" status={LIVE} rate={RATE} />);
  const captions = r.root.findAll(
    (n) =>
      (n.type as unknown) === "Text" && ["Source", "Speaker"].includes(n.props.children as string),
  );
  expect(captions).toHaveLength(2);
  for (const caption of captions) expect(caption.props.maxFontSizeMultiplier).toBe(1.3);
  expect(hostById(r, "led-format").props.maxFontSizeMultiplier).toBe(1.3);
});

test("empty rows ask for a choice, dimmed, and say so aloud", () => {
  const r = create(<LedDisplay source="" speaker="" status={STANDBY} rate={RATE} />);
  expect(hostById(r, "led-source").props.children).toBe("Choose a source");
  expect(flat(hostById(r, "led-source").props.style).color).toBe("#9a8f78");
  expect(hostById(r, "led-speaker").props.children).toBe("Choose a speaker");
  expect(flat(hostById(r, "led-speaker").props.style).color).toBe("#9a8f78");
  expect(hostById(r, "led-window").props.accessibilityLabel).toBe(
    "Status: Ready. No source chosen. No speaker chosen. 44.1 kilohertz stereo.",
  );
});

test("standby leaves the lamp off in warm brown-gray", () => {
  const r = create(<LedDisplay source="Spotify" speaker="Den" status={STANDBY} rate={RATE} />);
  expect(flat(hostById(r, "live-lamp").props.style)).toMatchObject({
    backgroundColor: "#3a332a",
    shadowOpacity: 0,
  });
  expect(hostById(r, "led-status").props.children).toBe("STANDBY");
});

test("an unknown sample rate is spoken as unknown", () => {
  const r = create(
    <LedDisplay
      source="Spotify"
      speaker="Den"
      status={STANDBY}
      rate={{ text: "--.-K", spoken: "sample rate unknown" }}
    />,
  );
  expect(hostById(r, "led-format").props.children).toBe("--.-K · ST");
  expect(hostById(r, "led-window").props.accessibilityLabel).toBe(
    "Status: Ready. Source Spotify. Speaker Den. Sample rate unknown.",
  );
});

test("a long name DSEG14 cannot draw stays on one line in Plex Mono", () => {
  const r = create(
    <LedDisplay
      source="Spotify"
      speaker="Küche – HomePod 🎵 Wohnzimmer Lautsprecher"
      status={LIVE}
      rate={RATE}
    />,
  );
  const speaker = hostById(r, "led-speaker");
  expect(flat(speaker.props.style).fontFamily).toBe("IBMPlexMono-Medium");
  expect(speaker.props.numberOfLines).toBe(1);
});
