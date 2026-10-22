import { act, renderHook } from "@testing-library/react-native";
import { useFaderDraft } from "@/hooks/useFaderDraft";

beforeEach(() => jest.useFakeTimers());
afterEach(() => jest.useRealTimers());

type Props = Parameters<typeof useFaderDraft>[0];

test("ignores prop updates while interacting and applies the latest after release", async () => {
  const onChange = jest.fn();
  const props = (value: number): Props => ({ value, min: 0, max: 100, step: 1, onChange });
  const { result, rerender } = renderHook((p: Props) => useFaderDraft(p), {
    initialProps: props(50),
  });

  act(() => result.current.preview(70.4));
  expect(result.current.draft).toBe(70);
  rerender(props(40));
  expect(result.current.draft).toBe(70);

  await act(async () => {
    jest.advanceTimersByTime(180);
  });
  expect(onChange).toHaveBeenCalledWith(70);
  rerender(props(45));
  expect(result.current.draft).toBe(70);

  await act(async () => {
    jest.advanceTimersByTime(300);
  });
  expect(result.current.draft).toBe(45);

  rerender(props(20));
  expect(result.current.draft).toBe(20);
});

test("a held gesture keeps the draft however long the finger rests, until it lets go", async () => {
  const onChange = jest.fn();
  const props = (value: number): Props => ({ value, min: 0, max: 100, step: 1, onChange });
  const { result, rerender } = renderHook((p: Props) => useFaderDraft(p), {
    initialProps: props(50),
  });
  act(() => result.current.hold());
  act(() => result.current.preview(80));
  await act(async () => {
    jest.advanceTimersByTime(180); // committed; its async tail would schedule the settle
  });
  await act(async () => {
    jest.advanceTimersByTime(800); // the finger is still down
  });
  expect(onChange).toHaveBeenCalledWith(80);
  rerender(props(20)); // another client moves the volume
  expect(result.current.draft).toBe(80);

  act(() => result.current.letGo());
  await act(async () => {
    jest.advanceTimersByTime(300);
  });
  expect(result.current.draft).toBe(20);
});

test("a new drag during the settle window keeps holding the value", async () => {
  const onChange = jest.fn();
  const props = (value: number): Props => ({ value, min: -12, max: 12, step: 0.5, onChange });
  const { result, rerender, unmount } = renderHook((p: Props) => useFaderDraft(p), {
    initialProps: props(0),
  });
  act(() => result.current.preview(3));
  await act(async () => {
    jest.advanceTimersByTime(180);
  });
  act(() => result.current.preview(4.2));
  await act(async () => {
    jest.advanceTimersByTime(300);
  });
  rerender(props(-1));
  expect(result.current.draft).toBe(4);
  await act(async () => {
    jest.advanceTimersByTime(180 + 300);
  });
  expect(onChange).toHaveBeenLastCalledWith(4);
  expect(result.current.draft).toBe(-1);
  act(() => result.current.preview(6));
  unmount();
  jest.advanceTimersByTime(1_000);
  expect(onChange).toHaveBeenCalledTimes(2);
});
