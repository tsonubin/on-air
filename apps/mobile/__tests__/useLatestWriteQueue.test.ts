import { act, renderHook } from "@testing-library/react-native";
import { useLatestWriteQueue } from "@/hooks/useLatestWriteQueue";

function deferred<T = void>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

const settle = () => act(async () => {});

test("sends immediately when idle and keeps only the newest value while busy", async () => {
  const first = deferred();
  const send = jest.fn<Promise<void>, [number]>().mockReturnValueOnce(first.promise);
  send.mockResolvedValue(undefined);
  const { result } = renderHook(() => useLatestWriteQueue(send));

  act(() => result.current.push(1));
  expect(send).toHaveBeenCalledTimes(1);
  act(() => {
    result.current.push(2);
    result.current.push(3);
  });
  expect(send).toHaveBeenCalledTimes(1);

  first.resolve();
  await settle();
  expect(send).toHaveBeenCalledTimes(2);
  expect(send).toHaveBeenNthCalledWith(2, 3);
});

test("reports a failure only for the newest write", async () => {
  const first = deferred();
  const send = jest.fn<Promise<void>, [number]>().mockReturnValueOnce(first.promise);
  send.mockResolvedValue(undefined);
  const onError = jest.fn();
  const { result } = renderHook(() => useLatestWriteQueue(send, onError));

  act(() => result.current.push(1));
  act(() => result.current.push(2));
  first.reject(new Error("lost"));
  await settle();
  expect(onError).not.toHaveBeenCalled();
  expect(send).toHaveBeenLastCalledWith(2);

  send.mockRejectedValueOnce(new Error("lost again"));
  act(() => result.current.push(3));
  await settle();
  expect(onError).toHaveBeenCalledTimes(1);
  expect(onError).toHaveBeenCalledWith(expect.any(Error), 3);
});

test("reset drops the pending value and silences in-flight failures", async () => {
  const first = deferred();
  const send = jest.fn<Promise<void>, [number]>().mockReturnValueOnce(first.promise);
  send.mockResolvedValue(undefined);
  const onError = jest.fn();
  const { result } = renderHook(() => useLatestWriteQueue(send, onError));

  act(() => result.current.push(1));
  act(() => result.current.push(2));
  act(() => result.current.reset());
  first.reject(new Error("stale"));
  await settle();
  expect(onError).not.toHaveBeenCalled();
  expect(send).toHaveBeenCalledTimes(1);
});

test("uses the latest sender without re-queuing", async () => {
  const sendA = jest.fn().mockResolvedValue(undefined);
  const sendB = jest.fn().mockResolvedValue(undefined);
  const { result, rerender } = renderHook(
    ({ send }: { send: (value: number) => Promise<void> }) => useLatestWriteQueue<number>(send),
    {
      initialProps: { send: sendA },
    },
  );
  rerender({ send: sendB });
  act(() => result.current.push(7));
  await settle();
  expect(sendA).not.toHaveBeenCalled();
  expect(sendB).toHaveBeenCalledWith(7);
});
