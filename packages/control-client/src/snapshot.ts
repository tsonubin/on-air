/**
 * The core's state as a client sees it, and the pure pieces both apps use
 * to keep it current: slice readers, result merging with a staleness
 * guard, the verdict on a refresh, and the WebSocket event reducer.
 */
import type {
  ActiveInputResponse,
  ActiveOutputView,
  AirPlayMode,
  AudioQualityResponse,
  CdStatus,
  EqGains,
  MeterLevel,
  OutputInfo,
  SampleRateResponse,
  StatusResponse,
  SyncResponse,
  WsEvent,
} from "@on-air/api-types";
import { isServicePaused, isUnauthorized } from "./errorCopy.ts";
import { HttpError } from "./http.ts";

/** Everything a UI reads from the core. Each field is `null` until its first fetch lands. */
export interface CoreSnapshot {
  status: StatusResponse | null;
  inputs: string[] | null;
  activeInput: string | null;
  outputs: OutputInfo[] | null;
  /** `state` is absent on older cores; treat that as live. */
  activeOutput: ActiveOutputView | null;
  volume: number | null;
  gains: EqGains | null;
  sampleRate: SampleRateResponse | null;
  /** DSP quality profile: current, recommended and selectable. */
  audioQuality: AudioQualityResponse | null;
  /** Per-output latency trim and sync state. */
  sync: SyncResponse | null;
  /** Loopback-only endpoint: the desktop shows it, the phone never asks. */
  pin: string | null;
  airplayMode: AirPlayMode | null;
  cd: CdStatus | null;
}

export const EMPTY_SNAPSHOT: CoreSnapshot = {
  status: null,
  inputs: null,
  activeInput: null,
  outputs: null,
  activeOutput: null,
  volume: null,
  gains: null,
  sampleRate: null,
  audioQuality: null,
  sync: null,
  pin: null,
  airplayMode: null,
  cd: null,
};

/** One endpoint's worth of snapshot; the unit a refresh is asked for. */
export type Slice = keyof CoreSnapshot;

export const DEVICE_SLICES: readonly Slice[] = [
  "inputs",
  "activeInput",
  "outputs",
  "activeOutput",
  "cd",
  "sync",
];
export const CONFIG_SLICES: readonly Slice[] = [
  "volume",
  "gains",
  "sampleRate",
  "audioQuality",
  "airplayMode",
];
/** Every slice a LAN client may read. `pin` is loopback-only; the desktop adds it itself. */
export const ALL_SLICES: readonly Slice[] = ["status", ...DEVICE_SLICES, ...CONFIG_SLICES];
/** A failed required slice is reported; one of these failing is "partly refreshed". */
export const REQUIRED_SLICES: readonly Slice[] = [
  "status",
  "inputs",
  "outputs",
  "activeInput",
  "activeOutput",
  "volume",
  "gains",
  "sampleRate",
];
/** Older cores lack these endpoints; a failure here is not an error. */
export const OPTIONAL_SLICES: readonly Slice[] = [
  "airplayMode",
  "cd",
  "pin",
  "audioQuality",
  "sync",
];

/** What each slice's endpoint returns. */
export interface SliceValue {
  status: StatusResponse;
  inputs: string[];
  activeInput: ActiveInputResponse;
  outputs: OutputInfo[];
  activeOutput: ActiveOutputView | null;
  volume: number;
  gains: EqGains;
  sampleRate: SampleRateResponse;
  audioQuality: AudioQualityResponse;
  sync: SyncResponse;
  pin: string;
  airplayMode: AirPlayMode;
  cd: CdStatus;
}

export type SliceReaders = { [K in Slice]: () => Promise<SliceValue[K]> };

/** The read half of the HTTP client, as `sliceReaders` needs it. */
export interface SliceApi {
  fetchStatus(base: string): Promise<StatusResponse>;
  listInputs(base: string, token?: string): Promise<string[]>;
  getActiveInput(base: string, token?: string): Promise<ActiveInputResponse>;
  listOutputs(base: string, token?: string): Promise<OutputInfo[]>;
  getActiveOutput(base: string, token?: string): Promise<ActiveOutputView | null>;
  getVolume(base: string, token?: string): Promise<number>;
  getEq(base: string, token?: string): Promise<EqGains>;
  getSampleRate(base: string, token?: string): Promise<SampleRateResponse>;
  getAudioQuality(base: string, token?: string): Promise<AudioQualityResponse>;
  getSync(base: string, token?: string): Promise<SyncResponse>;
  getPairingPin(base: string): Promise<string>;
  getAirplayMode(base: string, token?: string): Promise<AirPlayMode>;
  getCd(base: string, token?: string): Promise<CdStatus>;
}

/** Readers bound to one core. `api` is injected so apps can mock the client module. */
export function sliceReaders(api: SliceApi, base: string, token?: string): SliceReaders {
  return {
    status: () => api.fetchStatus(base),
    inputs: () => api.listInputs(base, token),
    activeInput: () => api.getActiveInput(base, token),
    outputs: () => api.listOutputs(base, token),
    activeOutput: () => api.getActiveOutput(base, token),
    volume: () => api.getVolume(base, token),
    gains: () => api.getEq(base, token),
    sampleRate: () => api.getSampleRate(base, token),
    audioQuality: () => api.getAudioQuality(base, token),
    sync: () => api.getSync(base, token),
    pin: () => api.getPairingPin(base),
    airplayMode: () => api.getAirplayMode(base, token),
    cd: () => api.getCd(base, token),
  };
}

export type SliceResults = { [K in Slice]?: PromiseSettledResult<SliceValue[K]> };

/**
 * Reads the requested slices. `status` goes first when asked for: a paused
 * core answers 503 to everything else, so the rest is skipped and the
 * verdict is "paused" without a burst of failed requests.
 */
export async function readSlices(
  readers: SliceReaders,
  slices: readonly Slice[],
): Promise<SliceResults> {
  const results: SliceResults = {};
  let rest = slices.filter((slice, index) => slices.indexOf(slice) === index);
  if (rest.includes("status")) {
    const [status] = await Promise.allSettled([readers.status()]);
    results.status = status;
    rest = rest.filter((slice) => slice !== "status");
    if (status.status === "fulfilled" && status.value.service_enabled === false) return results;
  }
  const settled = await Promise.allSettled(rest.map((slice) => readers[slice]()));
  rest.forEach((slice, index) => {
    (results as Record<Slice, PromiseSettledResult<unknown>>)[slice] = settled[index];
  });
  return results;
}

export function fulfilled<K extends Slice>(
  results: SliceResults,
  slice: K,
): SliceValue[K] | undefined {
  const result = results[slice];
  return result?.status === "fulfilled" ? (result.value as SliceValue[K]) : undefined;
}

export function rejection(results: SliceResults, slice: Slice): unknown {
  const result = results[slice];
  return result?.status === "rejected" ? result.reason : undefined;
}

/** The core answered, even if with an error status. */
function reached(results: SliceResults, slice: Slice): boolean {
  const result = results[slice];
  if (!result) return false;
  return result.status === "fulfilled" || result.reason instanceof HttpError;
}

/**
 * Folds fulfilled, non-stale slices into the snapshot; rejected or stale
 * ones leave it untouched. `isStale` says whether an event or local patch
 * set the slice after the read started (see `createStaleGuard`).
 */
export function mergeResults(
  previous: CoreSnapshot,
  results: SliceResults,
  isStale: (slice: Slice) => boolean = () => false,
): CoreSnapshot {
  const next = { ...previous };
  for (const slice of Object.keys(results) as Slice[]) {
    const result = results[slice];
    if (!result || result.status !== "fulfilled" || isStale(slice)) continue;
    if (slice === "activeInput") {
      next.activeInput = (result.value as ActiveInputResponse).name ?? null;
    } else {
      (next as Record<Slice, unknown>)[slice] = result.value;
    }
  }
  return next;
}

export type RefreshPhase = "connected" | "reconnecting" | "paused";

export type RefreshOutcome =
  | { kind: "unauthorized"; error: unknown }
  | { kind: "settled"; phase: RefreshPhase; partial: boolean }
  /** Only optional slices were asked for and none reached the core: keep the phase. */
  | { kind: "inconclusive" };

/**
 * Decides the connection phase from one refresh. Authentication loss beats
 * everything else; a paused service beats failures; a status read that got
 * no answer at all is an outage whatever else succeeded; otherwise
 * reachability is judged on the required slices when any were requested,
 * so a missing optional endpoint on an older core never reads as an outage.
 */
export function judgeResults(results: SliceResults, requested: readonly Slice[]): RefreshOutcome {
  for (const slice of requested) {
    const reason = rejection(results, slice);
    if (isUnauthorized(reason)) return { kind: "unauthorized", error: reason };
  }
  const status = fulfilled(results, "status");
  const paused =
    status?.service_enabled === false ||
    requested.some((slice) => isServicePaused(rejection(results, slice)));
  if (paused) return { kind: "settled", phase: "paused", partial: false };
  if (
    requested.includes("status") &&
    results.status?.status === "rejected" &&
    !reached(results, "status")
  ) {
    return { kind: "settled", phase: "reconnecting", partial: false };
  }
  const required = requested.filter((slice) => REQUIRED_SLICES.includes(slice));
  const considered = required.length > 0 ? required : requested;
  if (!considered.some((slice) => reached(results, slice))) {
    return required.length > 0
      ? { kind: "settled", phase: "reconnecting", partial: false }
      : { kind: "inconclusive" };
  }
  return {
    kind: "settled",
    phase: "connected",
    partial: required.some((slice) => results[slice]?.status === "rejected"),
  };
}

/**
 * Generation stamps for "did an event or local patch set this slice after
 * my read started?". Every `touch` bumps the generation; a read remembers
 * `now()` when it starts and drops slices stamped later, so a poll that
 * left before an event cannot land after it and restore stale state.
 */
export interface StaleGuard<K> {
  touch(...keys: K[]): void;
  now(): number;
  isStale(key: K, since: number): boolean;
  reset(): void;
}

export function createStaleGuard<K extends string>(): StaleGuard<K> {
  let generation = 0;
  const touchedAt = new Map<K, number>();
  return {
    touch(...keys) {
      generation += 1;
      for (const key of keys) touchedAt.set(key, generation);
    },
    now: () => generation,
    isStale: (key, since) => (touchedAt.get(key) ?? 0) > since,
    reset() {
      touchedAt.clear();
    },
  };
}

/**
 * Fields a `DeviceJoined` event does not carry. A refresh of `outputs`
 * follows every join to replace these guesses with the catalog entry.
 */
function provisionalOutput(ev: Extract<WsEvent, { type: "DeviceJoined" }>): OutputInfo {
  return {
    id: ev.id,
    name: ev.name,
    transport: ev.transport,
    kind: "solo",
    member_count: 1,
    needs_pair: ev.transport === "airplay",
    paired: ev.transport !== "airplay",
  };
}

/** One `LevelMeter` frame: both channels of one moment, straight to the UI. */
export interface StereoLevels {
  left: MeterLevel;
  right: MeterLevel;
}

export interface EventReduction {
  next: CoreSnapshot;
  /** Slices the event set; stamp them so an older read cannot overwrite them. */
  touched: Slice[];
  /** Slices worth re-reading because the event does not carry everything. */
  refresh: Slice[];
  /** The live input died on its own; the UI should say so. */
  inputStopped: boolean;
  /** A `ServiceStateChanged` arrived with this value. */
  serviceEnabled?: boolean;
  /**
   * A `LevelMeter` event's reading. Levels are too frequent for the
   * snapshot: they never touch a slice and never trigger a read.
   */
  meter?: StereoLevels;
}

/** Applies one WebSocket event to the snapshot. Pure; the hooks do the side effects. */
export function reduceEvent(prev: CoreSnapshot, ev: WsEvent): EventReduction {
  const unchanged: EventReduction = { next: prev, touched: [], refresh: [], inputStopped: false };
  switch (ev.type) {
    case "CdStateChanged": {
      const { type: _type, ...fields } = ev;
      const wasPresent = prev.cd?.present ?? false;
      const next = { ...prev, cd: { ...(prev.cd ?? { tracks: [] }), ...fields } };
      // A disc arriving or leaving adds or removes the "Audio CD" input, and
      // autoplay may switch to it. A track change carries everything itself.
      const refresh: Slice[] = ev.present !== wasPresent ? ["cd", "inputs", "activeInput"] : [];
      return { ...unchanged, next, touched: ["cd"], refresh };
    }
    case "InputStateChanged": {
      let next = prev;
      if (ev.active) next = { ...prev, activeInput: ev.name };
      else if (ev.name === null || prev.activeInput === ev.name)
        next = { ...prev, activeInput: null };
      // Switching input can change the catalog and the input rate.
      return {
        ...unchanged,
        next,
        touched: ["activeInput"],
        refresh: ["inputs", "activeInput", "sampleRate"],
        inputStopped: Boolean(ev.error),
      };
    }
    case "ServiceStateChanged": {
      const next = prev.status
        ? { ...prev, status: { ...prev.status, service_enabled: ev.enabled } }
        : prev;
      // Events during a pause are not sent; one full refresh catches up.
      const refresh: Slice[] = ev.enabled ? [...ALL_SLICES] : [];
      return { ...unchanged, next, touched: ["status"], refresh, serviceEnabled: ev.enabled };
    }
    case "OutputStateChanged": {
      if (!ev.active) {
        // A stop for another transport means a switch already replaced it.
        const next =
          prev.activeOutput?.transport === ev.transport ? { ...prev, activeOutput: null } : prev;
        return {
          ...unchanged,
          next,
          touched: ["activeOutput"],
          refresh: ["sync"],
        };
      }
      // The event names the device but does not carry its id; the catalog
      // usually has it, otherwise a refresh fills it in. The core only
      // announces an active output once it is live. Volume follows the output.
      const match = prev.outputs?.find(
        (o) => o.transport === ev.transport && o.name === ev.device_name,
      );
      const next: CoreSnapshot = {
        ...prev,
        activeOutput: {
          transport: ev.transport,
          device_name: ev.device_name,
          device_id: match?.id ?? "",
          state: "live",
        },
      };
      return {
        ...unchanged,
        next,
        touched: ["activeOutput"],
        refresh: match ? ["volume", "sync"] : ["activeOutput", "volume", "sync"],
      };
    }
    case "DeviceJoined": {
      const outputs = prev.outputs ?? [];
      const known = outputs.some((o) => o.transport === ev.transport && o.id === ev.id);
      const next = known ? prev : { ...prev, outputs: [...outputs, provisionalOutput(ev)] };
      return { ...unchanged, next, touched: ["outputs"], refresh: ["outputs", "sync"] };
    }
    case "DeviceLeft": {
      const next = {
        ...prev,
        outputs:
          prev.outputs?.filter((o) => !(o.transport === ev.transport && o.id === ev.id)) ?? null,
      };
      return { ...unchanged, next, touched: ["outputs"], refresh: ["sync"] };
    }
    case "SyncGroupChanged":
      // Groups were created, reshaped, started or stopped and the event
      // carries no state. Stamp `sync` so a read that left before this
      // event cannot land after it and restore the old groups, then re-read
      // it through the coalescing queue.
      return { ...unchanged, touched: ["sync"], refresh: ["sync"] };
    case "LevelMeter":
      // Not snapshot state: hand the reading straight to the UI.
      return { ...unchanged, meter: { left: ev.left, right: ev.right } };
    default:
      // Heartbeat and types this client does not know.
      return unchanged;
  }
}
