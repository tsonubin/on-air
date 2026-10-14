/**
 * Test doubles shared by the package's own tests, the desktop and the
 * phone app. Imported as `@on-air/control-client/testing`; never from
 * production code.
 */
import type { CdStatus, OutputInfo, SampleRateResponse, WsEvent } from "@on-air/api-types";
import type { WebSocketLike } from "./events.ts";

/** A scriptable WHATWG WebSocket: tests open it, push frames, and drop it. */
export class FakeSocket implements WebSocketLike {
  static instances: FakeSocket[] = [];
  /** When set, `new FakeSocket()` throws, like a runtime refusing the URL. */
  static failConstructor = false;

  onopen: WebSocketLike["onopen"] = null;
  onmessage: WebSocketLike["onmessage"] = null;
  onclose: WebSocketLike["onclose"] = null;
  onerror: WebSocketLike["onerror"] = null;
  closed = false;
  readonly url: string;
  private pingListeners: Array<() => void> = [];

  constructor(url: string) {
    if (FakeSocket.failConstructor) throw new Error("boom");
    this.url = url;
    FakeSocket.instances.push(this);
  }

  close(): void {
    this.closed = true;
  }

  on(_event: "ping", listener: () => void): void {
    this.pingListeners.push(listener);
  }

  // The server side of the script.

  open(): void {
    this.onopen?.({});
  }
  /** Delivers one frame: an event (sent as JSON) or raw data as is. */
  send(frame: WsEvent | string | ArrayBuffer): void {
    const data =
      typeof frame === "object" && !(frame instanceof ArrayBuffer) ? JSON.stringify(frame) : frame;
    this.onmessage?.({ data });
  }
  ping(): void {
    for (const listener of this.pingListeners) listener();
  }
  dropFromServer(): void {
    this.onclose?.({ code: 1006 });
  }
  fail(): void {
    this.onerror?.({});
  }

  static reset(): void {
    FakeSocket.instances = [];
    FakeSocket.failConstructor = false;
  }
  /** The most recently opened socket, if any. */
  static get last(): FakeSocket | undefined {
    return FakeSocket.instances[FakeSocket.instances.length - 1];
  }
  /** The most recently opened socket; throws when none was opened. */
  static latest(): FakeSocket {
    const socket = FakeSocket.last;
    if (!socket) throw new Error("no socket has been opened");
    return socket;
  }
}

/** Lets pending promise chains settle. */
export const tick = (ms = 0) => new Promise<void>((resolve) => setTimeout(resolve, ms));

/** A promise the test resolves by hand, for in-flight requests. */
export function deferred<T = void>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

/** Fixtures that mirror what the mock core (`ON_AIR_MOCK=1`) reports. */
export const sonos: OutputInfo = {
  id: "uuid:mock-sonos",
  name: "Mock Sonos",
  transport: "sonos",
  kind: "solo",
  member_count: 1,
  needs_pair: false,
  paired: true,
};

export const homepod: OutputInfo = {
  id: "hp-1",
  name: "Bedroom",
  transport: "airplay",
  kind: "pair",
  member_count: 2,
  needs_pair: true,
  paired: false,
};

export const idleCd: CdStatus = {
  present: false,
  playing: false,
  track: 0,
  track_count: 0,
  position_ms: 0,
  duration_ms: 0,
  tracks: [],
};

export const sampleRate: SampleRateResponse = {
  sample_rate_hz: 44100,
  input: { sample_rate_hz: 44100, supported_hz: [44100, 48000] },
  output: { sample_rate_hz: 48000, supported_hz: [44100, 48000, 96000], transport: "sonos" },
};
