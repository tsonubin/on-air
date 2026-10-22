import type { ReactElement } from "react";
import ReactTestRenderer, { type ReactTestInstance } from "react-test-renderer";
import { EqPanel, eqTrackLength } from "@/eq-panel";

const { act } = ReactTestRenderer;

function create(element: ReactElement) {
  let renderer!: ReactTestRenderer.ReactTestRenderer;
  act(() => {
    renderer = ReactTestRenderer.create(element);
  });
  return renderer;
}

const hostById = (r: ReactTestRenderer.ReactTestRenderer, id: string) =>
  r.root.find((n) => typeof n.type === "string" && n.props.testID === id);
const textsIn = (node: ReactTestInstance) =>
  node.findAll((n) => (n.type as unknown) === "Text" && typeof n.props.children === "string");

afterEach(() => jest.useRealTimers());

test("five vertical faders with spoken band names and LED dB readouts", () => {
  const r = create(
    <EqPanel gains={[3.5, 0, -2, 0, 12]} disabled={false} onChangeBand={jest.fn()} />,
  );
  expect(hostById(r, "eq-band-0").props).toMatchObject({
    accessibilityRole: "adjustable",
    accessibilityLabel: "Bass, 60 hertz",
    accessibilityValue: { min: -12, max: 12, now: 3.5, text: "plus 3.5 decibels" },
  });
  expect(hostById(r, "eq-band-2").props.accessibilityValue.text).toBe("minus 2 decibels");
  expect(hostById(r, "eq-band-4").props.accessibilityLabel).toBe("Air, 12 kilohertz");
  expect(textsIn(hostById(r, "eq-band-0-readout")).map((t) => t.props.children)).toContain("+3.5");
  expect(hostById(r, "eq-curve").props.children).toBe("CUSTOM");
});

test("a flat curve reads FLAT", () => {
  const r = create(<EqPanel gains={[0, 0, 0, 0, 0]} disabled={false} onChangeBand={jest.fn()} />);
  expect(hostById(r, "eq-curve").props.children).toBe("FLAT");
});

test("an increment commits the stepped band", async () => {
  jest.useFakeTimers();
  const onChangeBand = jest.fn();
  const r = create(
    <EqPanel gains={[0, 0, 0, 0, 0]} disabled={false} onChangeBand={onChangeBand} />,
  );
  act(() =>
    hostById(r, "eq-band-2").props.onAccessibilityAction({
      nativeEvent: { actionName: "increment" },
    }),
  );
  await act(async () => {
    jest.advanceTimersByTime(180);
  });
  expect(onChangeBand).toHaveBeenCalledWith(2, 1);
});

test("disabled bands report it and take no input", () => {
  const r = create(<EqPanel gains={[0, 0, 0, 0, 0]} disabled onChangeBand={jest.fn()} />);
  for (let i = 0; i < 5; i += 1) {
    expect(hostById(r, `eq-band-${i}`).props.accessibilityState).toEqual({ disabled: true });
  }
});

test("track length fills the row within bounds", () => {
  expect(eqTrackLength(100)).toBe(140);
  expect(eqTrackLength(300)).toBe(204);
  expect(eqTrackLength(1000)).toBe(320);
});

test("the scale, 0 dB line and readouts are decoration", () => {
  const r = create(<EqPanel gains={[0, 0, 0, 0, 0]} disabled={false} onChangeBand={jest.fn()} />);
  expect(hostById(r, "eq-scale").props.accessibilityElementsHidden).toBe(true);
  expect(hostById(r, "eq-zero-line").props.accessibilityElementsHidden).toBe(true);
  // The fader itself speaks the value, so its LED readout is hidden.
  const readout = hostById(r, "eq-band-0-readout");
  let hidden = false;
  for (let node: ReactTestInstance | null = readout; node; node = node.parent) {
    if (node.props.accessibilityElementsHidden) hidden = true;
  }
  expect(hidden).toBe(true);
});

test("the footnote explains the faders", () => {
  const r = create(<EqPanel gains={[0, 0, 0, 0, 0]} disabled={false} onChangeBand={jest.fn()} />);
  expect(textsIn(r.root).map((t) => t.props.children)).toContain(
    "Drag up to boost, down to cut. 0 dB leaves the sound unchanged.",
  );
});
