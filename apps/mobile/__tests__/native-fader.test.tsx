import { act, render } from "@testing-library/react-native";
import { NativeFader } from "@/native-fader";

beforeEach(() => jest.useFakeTimers());
afterEach(() => jest.useRealTimers());

function slider(view: ReturnType<typeof render>) {
  return view.UNSAFE_getByProps({ testID: "volume-native" });
}

test("ignores prop updates while interacting and applies the latest after release", async () => {
  const onChange = jest.fn();
  const element = (value: number) => (
    <NativeFader
      label="Volume"
      accessibilityLabel="Output volume"
      value={value}
      min={0}
      max={100}
      testId="volume"
      onChange={onChange}
    />
  );
  const view = render(element(50));
  expect(slider(view).props.modifiers).toEqual([
    { $type: "accessibilityLabel", label: "Output volume" },
  ]);

  act(() => slider(view).props.onValueChange(70.4));
  expect(slider(view).props.value).toBe(70);
  view.rerender(element(40));
  expect(slider(view).props.value).toBe(70);

  await act(async () => {
    jest.advanceTimersByTime(180);
  });
  expect(onChange).toHaveBeenCalledWith(70);
  view.rerender(element(45));
  expect(slider(view).props.value).toBe(70);

  await act(async () => {
    jest.advanceTimersByTime(300);
  });
  expect(slider(view).props.value).toBe(45);

  view.rerender(element(20));
  expect(slider(view).props.value).toBe(20);
});

test("a new drag during the settle window keeps holding the thumb", async () => {
  const onChange = jest.fn();
  const element = (value: number) => (
    <NativeFader
      label="Bass"
      value={value}
      min={-12}
      max={12}
      step={0.5}
      testId="volume"
      onChange={onChange}
    />
  );
  const view = render(element(0));
  act(() => slider(view).props.onValueChange(3));
  await act(async () => {
    jest.advanceTimersByTime(180);
  });
  act(() => slider(view).props.onValueChange(4.2));
  await act(async () => {
    jest.advanceTimersByTime(300);
  });
  view.rerender(element(-1));
  expect(slider(view).props.value).toBe(4);
  await act(async () => {
    jest.advanceTimersByTime(180 + 300);
  });
  expect(onChange).toHaveBeenLastCalledWith(4);
  expect(slider(view).props.value).toBe(-1);
  act(() => slider(view).props.onValueChange(6));
  view.unmount();
  jest.advanceTimersByTime(1_000);
  expect(onChange).toHaveBeenCalledTimes(2);
});
