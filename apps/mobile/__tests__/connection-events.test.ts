import { subscribeToDesktop } from "../src/connection-events";

const originalSocket = global.WebSocket;
let sockets: FakeSocket[] = [];
class FakeSocket {
  onopen: (() => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;
  close = jest.fn();
  constructor() {
    sockets.push(this);
  }
}
beforeEach(() => {
  jest.useFakeTimers();
  sockets = [];
  global.WebSocket = FakeSocket as unknown as typeof WebSocket;
});
afterEach(() => {
  global.WebSocket = originalSocket;
  jest.useRealTimers();
});

test("reconnects after Wi-Fi interruption and stops retrying when backgrounded", () => {
  const onConnected = jest.fn();
  const onInterrupted = jest.fn();
  const onMessage = jest.fn();
  const stop = subscribeToDesktop("ws://desktop/api/ws", { onConnected, onInterrupted, onMessage });
  sockets[0].onopen?.();
  sockets[0].onclose?.();
  expect(onInterrupted).toHaveBeenCalledTimes(1);
  jest.advanceTimersByTime(1000);
  expect(sockets).toHaveLength(2);
  sockets[1].onopen?.();
  sockets[1].onmessage?.({ data: "new state" });
  expect(onConnected).toHaveBeenCalledTimes(2);
  expect(onMessage).toHaveBeenCalledWith("new state");
  sockets[1].onerror?.();
  stop();
  jest.advanceTimersByTime(30_000);
  expect(sockets).toHaveLength(2);
});

test("backs off repeated failures and ignores events from a replaced socket", () => {
  const callbacks = { onConnected: jest.fn(), onInterrupted: jest.fn(), onMessage: jest.fn() };
  const stop = subscribeToDesktop("ws://desktop/api/ws", callbacks);
  const staleMessage = sockets[0].onmessage;
  sockets[0].onerror?.();
  jest.advanceTimersByTime(1000);
  sockets[1].onerror?.();
  jest.advanceTimersByTime(1000);
  expect(sockets).toHaveLength(2);
  jest.advanceTimersByTime(1000);
  expect(sockets).toHaveLength(3);
  staleMessage?.({ data: "stale" });
  expect(callbacks.onMessage).not.toHaveBeenCalled();
  stop();
});
