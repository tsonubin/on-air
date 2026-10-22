import type { ReactElement } from "react";
import ReactTestRenderer from "react-test-renderer";
import { dragValue, Fader, type FaderProps } from "@/ui/fader";

const { act } = ReactTestRenderer;
const onChange = jest.fn();

beforeEach(() => {
  onChange.mockReset();
  jest.useFakeTimers();
});
afterEach(() => jest.useRealTimers());

// Style props are arbitrarily nested arrays of objects; flatten like StyleSheet.flatten.
const flat = (style: any): Record<string, unknown> =>
  Array.isArray(style) ? Object.assign({}, ...style.filter(Boolean).map(flat)) : (style ?? {});

function create(element: ReactElement) {
  let renderer!: ReactTestRenderer.ReactTestRenderer;
  act(() => {
    renderer = ReactTestRenderer.create(element);
  });
  return renderer;
}

const node = (r: ReactTestRenderer.ReactTestRenderer, id: string) =>
  r.root.find((n) => typeof n.type === "string" && n.props.testID === id);
const exists = (r: ReactTestRenderer.ReactTestRenderer, id: string) =>
  r.root.findAll((n) => typeof n.type === "string" && n.props.testID === id).length > 0;
const touch = (x: number, y = 0) => ({ nativeEvent: { pageX: x, pageY: y } });
const layout = (width: number, height: number) => ({
  nativeEvent: { layout: { x: 0, y: 0, width, height } },
});
const now = (r: ReactTestRenderer.ReactTestRenderer, id: string) =>
  node(r, id).props.accessibilityValue.now as number;

const volume = (over: Partial<FaderProps> = {}) => (
  <Fader
    orientation="horizontal"
    value={50}
    min={0}
    max={100}
    step={1}
    origin={0}
    accessibilityStep={5}
    accessibilityLabel="Volume"
    formatValue={String}
    testID="vol"
    onChange={onChange}
    {...over}
  />
);

const band = (over: Partial<FaderProps> = {}) => (
  <Fader
    orientation="vertical"
    value={0}
    min={-12}
    max={12}
    step={0.5}
    origin={0}
    length={240}
    accessibilityStep={1}
    accessibilityLabel="Bass, 60 hertz"
    formatValue={(v) => (v === 2.5 ? "plus 2.5 decibels" : String(v))}
    testID="band"
    onChange={onChange}
    {...over}
  />
);

test("drags relative to the touch, snapping and clamping", async () => {
  const r = create(volume());
  act(() => node(r, "vol-track").props.onLayout(layout(300, 44)));
  const root = node(r, "vol");
  act(() => root.props.onResponderGrant(touch(10)));
  expect(now(r, "vol")).toBe(50); // no jump to the touch point
  act(() => root.props.onResponderMove(touch(79))); // +69 px of 300 → +23
  expect(now(r, "vol")).toBe(73);
  act(() => root.props.onResponderMove(touch(600)));
  expect(now(r, "vol")).toBe(100);
  act(() => root.props.onResponderRelease(touch(600)));
  await act(async () => {
    jest.advanceTimersByTime(180);
  });
  expect(onChange).toHaveBeenLastCalledWith(100);
});

test("vertical drags up to boost", () => {
  const r = create(band());
  const root = node(r, "band");
  act(() => root.props.onResponderGrant(touch(0, 0)));
  act(() => root.props.onResponderMove(touch(0, -25))); // 25 px of 240 → +2.5 dB
  expect(node(r, "band").props.accessibilityValue).toMatchObject({
    now: 2.5,
    text: "plus 2.5 decibels",
  });
});

test("fill runs from the origin", () => {
  expect(flat(node(create(band({ value: 6 })), "band-fill").props.style)).toMatchObject({
    top: "25%",
    height: "25%",
  });
  expect(flat(node(create(band({ value: -6 })), "band-fill").props.style)).toMatchObject({
    top: "50%",
    height: "25%",
  });
  expect(flat(node(create(volume()), "vol-fill").props.style)).toMatchObject({
    left: "0%",
    width: "50%",
  });
});

test("the cap's index line runs across the cap in both orientations", () => {
  // Found on device: a column cap stretched the line's width, leaving a 0-tall line.
  const across = (r: ReactTestRenderer.ReactTestRenderer, id: string) => ({
    cap: flat(node(r, `${id}-cap`).props.style),
    line: flat(node(r, `${id}-cap-line`).props.style),
  });
  const horizontal = across(create(volume()), "vol");
  expect(horizontal.cap.flexDirection).toBe("row");
  expect(horizontal.line).toMatchObject({ width: 1, alignSelf: "stretch" });
  const vertical = across(create(band()), "band");
  expect(vertical.cap.flexDirection).toBe("column");
  expect(vertical.line).toMatchObject({ height: 1, alignSelf: "stretch" });
});

test("a touch alone never changes the value", async () => {
  const r = create(volume());
  act(() => node(r, "vol-track").props.onLayout(layout(300, 44)));
  act(() => node(r, "vol").props.onResponderGrant(touch(200)));
  act(() => node(r, "vol").props.onResponderRelease(touch(200)));
  await act(async () => {
    jest.advanceTimersByTime(500);
  });
  expect(onChange).not.toHaveBeenCalled();
  expect(now(r, "vol")).toBe(50);
});

test("claims the touch and refuses to hand it over", () => {
  const root = node(create(volume()), "vol");
  expect(root.props.onStartShouldSetResponder(touch(0))).toBe(true);
  expect(root.props.onResponderTerminationRequest(touch(0))).toBe(false);
});

test("accessibility: adjustable with steps", async () => {
  const r = create(volume());
  expect(node(r, "vol").props).toMatchObject({
    accessible: true,
    accessibilityRole: "adjustable",
    accessibilityLabel: "Volume",
    accessibilityActions: [{ name: "increment" }, { name: "decrement" }],
    accessibilityValue: { min: 0, max: 100, now: 50, text: "50" },
  });
  act(() =>
    node(r, "vol").props.onAccessibilityAction({ nativeEvent: { actionName: "increment" } }),
  );
  expect(now(r, "vol")).toBe(55);
  await act(async () => {
    jest.advanceTimersByTime(180);
  });
  expect(onChange).toHaveBeenCalledWith(55);
});

test("disabled ignores touches and actions", () => {
  const r = create(volume({ disabled: true }));
  const root = node(r, "vol");
  expect(root.props.onStartShouldSetResponder).toBeUndefined();
  expect(root.props.onAccessibilityAction).toBeUndefined();
  expect(root.props.accessibilityState).toEqual({ disabled: true });
  expect(exists(r, "vol-fill")).toBe(false);
  expect(flat(node(r, "vol-cap").props.style).opacity).toBe(0.45);
});

test("a desktop value mid-drag never moves the cap", () => {
  const r = create(volume());
  act(() => node(r, "vol-track").props.onLayout(layout(300, 44)));
  act(() => node(r, "vol").props.onResponderGrant(touch(0)));
  act(() => node(r, "vol").props.onResponderMove(touch(90)));
  expect(now(r, "vol")).toBe(80);
  act(() => r.update(volume({ value: 20 })));
  expect(now(r, "vol")).toBe(80);
});

test("a finger resting on the cap keeps it there while the desktop changes the value", async () => {
  // Found in review: the hold used to lapse 480 ms after the last move, finger still down.
  const r = create(volume());
  act(() => node(r, "vol-track").props.onLayout(layout(300, 44)));
  act(() => node(r, "vol").props.onResponderGrant(touch(0)));
  act(() => node(r, "vol").props.onResponderMove(touch(90)));
  // Commit first (its async tail schedules the settle), then rest well past the settle.
  await act(async () => {
    jest.advanceTimersByTime(180);
  });
  await act(async () => {
    jest.advanceTimersByTime(500);
  });
  act(() => r.update(volume({ value: 20 })));
  expect(now(r, "vol")).toBe(80);
  act(() => node(r, "vol").props.onResponderRelease(touch(90)));
  await act(async () => {
    jest.advanceTimersByTime(300);
  });
  expect(now(r, "vol")).toBe(20);
});

test("disabling mid-hold lets go, so desktop values apply again", async () => {
  const r = create(volume());
  act(() => node(r, "vol-track").props.onLayout(layout(300, 44)));
  act(() => node(r, "vol").props.onResponderGrant(touch(0)));
  act(() => node(r, "vol").props.onResponderMove(touch(30)));
  act(() => r.update(volume({ disabled: true }))); // no release reaches a disabled fader
  await act(async () => {
    jest.advanceTimersByTime(180);
  });
  await act(async () => {
    jest.advanceTimersByTime(300);
  });
  act(() => r.update(volume({ disabled: true, value: 20 })));
  expect(now(r, "vol")).toBe(20);
});

test("the shown value is reported as it moves, clamped like the cap", () => {
  const onDraft = jest.fn();
  const r = create(volume({ onDraft }));
  expect(onDraft).toHaveBeenLastCalledWith(50);
  act(() =>
    node(r, "vol").props.onAccessibilityAction({ nativeEvent: { actionName: "increment" } }),
  );
  expect(onDraft).toHaveBeenLastCalledWith(55);
  create(volume({ value: 120, onDraft }));
  expect(onDraft).toHaveBeenLastCalledWith(100);
});

test("disabling mid-drag ignores further moves", async () => {
  const r = create(volume());
  act(() => node(r, "vol-track").props.onLayout(layout(300, 44)));
  act(() => node(r, "vol").props.onResponderGrant(touch(0)));
  const move = node(r, "vol").props.onResponderMove;
  act(() => move(touch(30)));
  expect(now(r, "vol")).toBe(60);
  act(() => r.update(volume({ disabled: true })));
  act(() => move(touch(150)));
  expect(flat(node(r, "vol-cap").props.style).left).toBe("60%");
  await act(async () => {
    jest.advanceTimersByTime(500);
  });
  expect(onChange.mock.calls).toEqual([[60]]);
});

test("touches before layout are ignored; out-of-range values pin the cap", () => {
  const r = create(volume());
  act(() => node(r, "vol").props.onResponderGrant(touch(0)));
  act(() => node(r, "vol").props.onResponderMove(touch(200)));
  expect(now(r, "vol")).toBe(50);
  expect(Number.isFinite(now(r, "vol"))).toBe(true);

  const loud = create(volume({ value: 120 }));
  expect(flat(node(loud, "vol-cap").props.style).left).toBe("100%");
  expect(now(loud, "vol")).toBe(100);
});

test("dragValue maps a drag across the track onto the range", () => {
  expect(dragValue({ start: 50, delta: 69, length: 300, min: 0, max: 100, step: 1 })).toBe(73);
  expect(dragValue({ start: 50, delta: 69, length: 0, min: 0, max: 100, step: 1 })).toBe(50);
  expect(dragValue({ start: 0, delta: -500, length: 240, min: -12, max: 12, step: 0.5 })).toBe(-12);
});
