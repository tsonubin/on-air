import {
  type ActiveInputResponse,
  type ActiveOutput,
  type AirPlayMode,
  API_BASE,
  type CdAction,
  type CdStatus,
  type EqGains,
  type OutputInfo,
  type SampleRateResponse,
  type SetSampleRateRequest,
  type StatusResponse,
  type Transport,
} from "@on-air/api-types";
import type { EventHandlers, EventSubscription, SubscribeOptions } from "@on-air/control-client";
import * as cc from "@on-air/control-client";

/**
 * The slice of `@on-air/control-client` the desktop uses, pre-bound to the
 * loopback core (no token: the core trusts loopback plus the origin
 * allow-list). Hooks take a `ClientLike` so tests can hand in fakes without
 * module mocking.
 */
export interface ClientLike {
  fetchStatus(): Promise<StatusResponse>;
  getPairingPin(): Promise<string>;
  listInputs(): Promise<string[]>;
  getActiveInput(): Promise<ActiveInputResponse>;
  activateInput(name: string): Promise<void>;
  listOutputs(): Promise<OutputInfo[]>;
  getActiveOutput(): Promise<ActiveOutput | null>;
  activateOutput(transport: Transport, deviceId: string): Promise<void>;
  deactivateOutput(): Promise<void>;
  getVolume(): Promise<number>;
  setVolume(volume: number): Promise<void>;
  getEq(): Promise<EqGains>;
  setEq(gains: EqGains): Promise<void>;
  getSampleRate(): Promise<SampleRateResponse>;
  setSampleRate(body: SetSampleRateRequest): Promise<void>;
  getAirplayMode(): Promise<AirPlayMode>;
  pairAirplay(deviceId: string, pin: string): Promise<void>;
  pairBluetooth(id: string): Promise<void>;
  openBluetoothSettings(): Promise<void>;
  getCd(): Promise<CdStatus>;
  controlCd(action: CdAction): Promise<CdStatus>;
  subscribeEvents(handlers: EventHandlers, opts?: SubscribeOptions): EventSubscription;
}

export function createClient(base: string = API_BASE): ClientLike {
  return {
    fetchStatus: () => cc.fetchStatus(base),
    getPairingPin: () => cc.getPairingPin(base),
    listInputs: () => cc.listInputs(base),
    getActiveInput: () => cc.getActiveInput(base),
    activateInput: (name) => cc.activateInput(base, name),
    listOutputs: () => cc.listOutputs(base),
    getActiveOutput: () => cc.getActiveOutput(base),
    activateOutput: (transport, deviceId) => cc.activateOutput(base, transport, deviceId),
    deactivateOutput: () => cc.deactivateOutput(base),
    getVolume: () => cc.getVolume(base),
    setVolume: (volume) => cc.setVolume(base, volume),
    getEq: () => cc.getEq(base),
    setEq: (gains) => cc.setEq(base, gains),
    getSampleRate: () => cc.getSampleRate(base),
    setSampleRate: (body) => cc.setSampleRate(base, body),
    getAirplayMode: () => cc.getAirplayMode(base),
    pairAirplay: (deviceId, pin) => cc.pairAirplay(base, deviceId, pin),
    pairBluetooth: (id) => cc.pairBluetooth(base, id),
    openBluetoothSettings: () => cc.openBluetoothSettings(base),
    getCd: () => cc.getCd(base),
    controlCd: (action) => cc.controlCd(base, action),
    subscribeEvents: (handlers, opts) => cc.subscribeEvents(base, undefined, handlers, opts),
  };
}

/** The one client the running app uses. */
export const liveClient: ClientLike = createClient();
