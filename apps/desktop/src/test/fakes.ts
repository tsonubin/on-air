import type {
  ActiveOutputView,
  AudioQualityResponse,
  CdStatus,
  DspQuality,
  EqGains,
  OutputInfo,
  OutputSyncStatus,
  SampleRateResponse,
  SetSyncTrimRequest,
  StatusResponse,
  SyncGroupMemberRequest,
  SyncGroupView,
  SyncResponse,
} from "@on-air/api-types";
import type { EventHandlers, SubscribeOptions } from "@on-air/control-client";
import { subscribeEvents } from "@on-air/control-client";
import { FakeSocket, homepod, idleCd, sampleRate, sonos } from "@on-air/control-client/testing";
import { type Mock, vi } from "vitest";
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

/** One output's sync row, as `GET /api/sync` reports it. */
export function mockSyncStatus(overrides: Partial<OutputSyncStatus> = {}): OutputSyncStatus {
  return {
    transport: "sonos",
    device_id: sonos.id,
    device_name: sonos.name,
    state: "locked",
    trim_ms: 0,
    default_latency_ms: 1000,
    effective_latency_ms: 1000,
    calibrated: false,
    delay_frames: 2400,
    output_sample_rate_hz: 48000,
    ...overrides,
  };
}

/** A two-speaker group playing together at the shared epoch. */
export const downstairs: SyncGroupView = {
  id: 1,
  name: "Downstairs",
  members: [
    { transport: "sonos", device_id: sonos.id, device_name: sonos.name, playing: true },
    { transport: "airplay", device_id: homepod.id, device_name: homepod.name, playing: true },
  ],
  playing: true,
};

/** An empty group that has not started. */
export const upstairs: SyncGroupView = {
  id: 2,
  name: "Upstairs",
  members: [],
  playing: false,
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
  audioQuality: AudioQualityResponse;
  sync: SyncResponse;
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
    audioQuality: {
      quality: "balanced",
      recommended: "balanced",
      profiles: ["economy", "balanced", "high"],
    },
    sync: { outputs: [] },
    pin: "123456",
    cd: idleCd,
    ...overrides,
  };
}

/** The fake core's sync groups, created on first use so fixtures can omit them. */
function syncGroups(core: FakeCore): SyncGroupView[] {
  core.sync.groups ??= [];
  return core.sync.groups;
}

/** Every `ClientLike` method replaced by a test double that answers from `core`. */
export type FakeClient = { [K in keyof ClientLike]: Mock<ClientLike[K]> };

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
    getAudioQuality: vi.fn(async () => core.audioQuality),
    setAudioQuality: vi.fn(async (quality: DspQuality) => {
      core.audioQuality = { ...core.audioQuality, quality };
    }),
    getSync: vi.fn(async () => core.sync),
    setSyncTrim: vi.fn(async (body: SetSyncTrimRequest) => {
      const row = core.sync.outputs.find(
        (o) => o.transport === body.transport && o.device_id === body.device_id,
      );
      const trim_ms = body.trim_ms;
      if (row) {
        row.trim_ms = trim_ms;
      } else {
        core.sync.outputs.push({
          transport: body.transport,
          device_id: body.device_id,
          device_name: body.device_id,
          state: "free_running",
          trim_ms,
          default_latency_ms: 0,
          effective_latency_ms: 0,
          calibrated: false,
          delay_frames: 0,
          output_sample_rate_hz: 48000,
        });
      }
      return { trim_ms, effective_latency_ms: Math.max(0, trim_ms) };
    }),
    createSyncGroup: vi.fn(async (name: string) => {
      const groups = syncGroups(core);
      const group: SyncGroupView = {
        id: groups.reduce((next, g) => Math.max(next, g.id), 0) + 1,
        name,
        members: [],
        playing: false,
      };
      groups.push(group);
      return group;
    }),
    dissolveSyncGroup: vi.fn(async (groupId: number) => {
      const groups = syncGroups(core);
      const gone = groups.find((g) => g.id === groupId);
      core.sync.groups = groups.filter((g) => g.id !== groupId);
      for (const member of gone?.members ?? []) {
        const row = core.sync.outputs.find(
          (o) => o.transport === member.transport && o.device_id === member.device_id,
        );
        if (row) delete row.group_id;
      }
    }),
    addSyncGroupMember: vi.fn(async (groupId: number, member: SyncGroupMemberRequest) => {
      syncGroups(core)
        .find((g) => g.id === groupId)
        ?.members.push({
          transport: member.transport,
          device_id: member.device_id,
          device_name:
            core.outputs.find((o) => o.transport === member.transport && o.id === member.device_id)
              ?.name ?? member.device_id,
          playing: false,
        });
      const row = core.sync.outputs.find(
        (o) => o.transport === member.transport && o.device_id === member.device_id,
      );
      if (row) row.group_id = groupId;
    }),
    leaveSyncGroupMember: vi.fn(async (groupId: number, member: SyncGroupMemberRequest) => {
      const group = syncGroups(core).find((g) => g.id === groupId);
      if (group) {
        group.members = group.members.filter(
          (m) => m.transport !== member.transport || m.device_id !== member.device_id,
        );
        group.playing = group.members.some((m) => m.playing);
      }
      const row = core.sync.outputs.find(
        (o) => o.transport === member.transport && o.device_id === member.device_id,
      );
      if (row) delete row.group_id;
    }),
    startSyncGroup: vi.fn(async (groupId: number) => {
      const group = syncGroups(core).find((g) => g.id === groupId);
      if (!group) return;
      for (const member of group.members) member.playing = true;
      group.playing = group.members.length > 0;
    }),
    stopSyncGroup: vi.fn(async (groupId: number) => {
      const group = syncGroups(core).find((g) => g.id === groupId);
      if (!group) return;
      for (const member of group.members) member.playing = false;
      group.playing = false;
    }),
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
