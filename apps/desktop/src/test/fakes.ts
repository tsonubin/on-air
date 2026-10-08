import type {
  ActiveOutputView,
  CdStatus,
  EqGains,
  OutputInfo,
  SampleRateResponse,
  StatusResponse,
  WsEvent,
} from "@on-air/api-types";
import type { EventHandlers, SubscribeOptions, WebSocketLike } from "@on-air/control-client";
import { subscribeEvents } from "@on-air/control-client";
import { vi } from "vitest";
import type { ClientLike } from "../lib/client";

/** Scriptable stand-in for a WHATWG WebSocket (same shape as the client's own tests). */
export class FakeSocket implements WebSocketLike {
  static instances: FakeSocket[] = [];
  onopen: WebSocketLike["onopen"] = null;
  onmessage: WebSocketLike["onmessage"] = null;
  onclose: WebSocketLike["onclose"] = null;
  onerror: WebSocketLike["onerror"] = null;
  closed = false;
  readonly url: string;

  constructor(url: string) {
    this.url = url;
    FakeSocket.instances.push(this);
  }

  close(): void {
    this.closed = true;
  }

  open(): void {
    this.onopen?.({});
  }
  send(event: WsEvent): void {
    this.onmessage?.({ data: JSON.stringify(event) });
  }
  dropFromServer(): void {
    this.onclose?.({ code: 1006 });
  }

  static reset(): void {
    FakeSocket.instances = [];
  }
  static latest(): FakeSocket {
    const socket = FakeSocket.instances[FakeSocket.instances.length - 1];
    if (!socket) throw new Error("no socket has been opened");
    return socket;
  }
}

export const sonos: OutputInfo = {
  id: "uuid:mock-sonos",
  name: "Kitchen",
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

/** Mutable backing store the fake client answers from. */
export interface FakeCore {
  status: StatusResponse;
  inputs: string[];
  activeInput: string | null;
  outputs: OutputInfo[];
  activeOutput: ActiveOutputView | null;
  volume: number;
  gains: EqGains;
  sampleRate: SampleRateResponse;
  pin: string;
  cd: CdStatus;
}

export function fakeCore(overrides: Partial<FakeCore> = {}): FakeCore {
  return {
    status: { status: "ok", version: "0.1.0", service_enabled: true, lan_addresses: ["10.0.0.2"] },
    inputs: ["Mock Monitor", "Line In"],
    activeInput: null,
    outputs: [sonos, homepod],
    activeOutput: null,
    volume: 50,
    gains: [0, 0, 0, 0, 0],
    sampleRate,
    pin: "123456",
    cd: idleCd,
    ...overrides,
  };
}

export type FakeClient = { [K in keyof ClientLike]: ReturnType<typeof vi.fn<ClientLike[K]>> };

/**
 * A `ClientLike` whose reads answer from `core` and whose writes resolve
 * immediately. Every method is a `vi.fn`, so tests can count calls or swap in
 * rejections and hanging promises.
 */
export function fakeClient(core: FakeCore, socketOpts: SubscribeOptions = {}): FakeClient {
  return {
    fetchStatus: vi.fn(async () => core.status),
    getPairingPin: vi.fn(async () => core.pin),
    listInputs: vi.fn(async () => core.inputs),
    getActiveInput: vi.fn(async () => ({
      name: core.activeInput,
      backend: "cpal-default" as const,
    })),
    activateInput: vi.fn(async (name: string) => {
      core.activeInput = name;
    }),
    listOutputs: vi.fn(async () => core.outputs),
    getActiveOutput: vi.fn(async () => core.activeOutput),
    activateOutput: vi.fn(async (transport, id) => {
      const output = core.outputs.find((o) => o.transport === transport && o.id === id);
      core.activeOutput = { transport, device_id: id, device_name: output?.name ?? id };
    }),
    deactivateOutput: vi.fn(async () => {
      core.activeOutput = null;
    }),
    getVolume: vi.fn(async () => core.volume),
    setVolume: vi.fn(async (v: number) => {
      core.volume = v;
    }),
    getEq: vi.fn(async () => core.gains),
    setEq: vi.fn(async (g: EqGains) => {
      core.gains = g;
    }),
    getSampleRate: vi.fn(async () => core.sampleRate),
    setSampleRate: vi.fn(async () => {}),
    getAirplayMode: vi.fn(async () => "owntone" as const),
    pairAirplay: vi.fn(async () => {}),
    pairBluetooth: vi.fn(async () => {}),
    openBluetoothSettings: vi.fn(async () => {}),
    getCd: vi.fn(async () => core.cd),
    controlCd: vi.fn(async () => core.cd),
    subscribeEvents: vi.fn((handlers: EventHandlers, opts?: SubscribeOptions) =>
      subscribeEvents("http://127.0.0.1:47990", undefined, handlers, {
        WebSocket: FakeSocket,
        minBackoffMs: 5,
        maxBackoffMs: 10,
        ...socketOpts,
        ...opts,
      }),
    ),
  };
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
