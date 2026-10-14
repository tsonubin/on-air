import type {
  ActiveOutputView,
  CdStatus,
  EqGains,
  OutputInfo,
  SampleRateResponse,
  StatusResponse,
} from "@on-air/api-types";
import type { EventHandlers, SubscribeOptions } from "@on-air/control-client";
import { subscribeEvents } from "@on-air/control-client";
import { FakeSocket, homepod, idleCd, sampleRate, sonos } from "@on-air/control-client/testing";
import { vi } from "vitest";
import type { ClientLike } from "../lib/client";

export {
  deferred,
  FakeSocket,
  homepod,
  idleCd,
  sampleRate,
  sonos,
  tick,
} from "@on-air/control-client/testing";

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
