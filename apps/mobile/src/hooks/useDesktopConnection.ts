import type {
  ActiveInputResponse,
  ActiveOutput,
  AirPlayMode,
  CdStatus,
  EqGains,
  OutputInfo,
  SampleRateResponse,
  StatusResponse,
  WsEvent,
} from "@on-air/api-types";
import {
  type ConnectionState,
  fetchStatus,
  getActiveInput,
  getActiveOutput,
  getAirplayMode,
  getCd,
  getEq,
  getSampleRate,
  getVolume,
  HttpError,
  listInputs,
  listOutputs,
  subscribeEvents,
  type WebSocketConstructor,
} from "@on-air/control-client";
import { useCallback, useEffect, useRef, useState } from "react";
import { friendlyError, isPaused, isUnauthorized } from "@/errors";

/**
 * `idle`: no pairing. `connecting`: first refresh for this pairing is out.
 * `connected`: the last refresh reached the desktop. `reconnecting`: it did
 * not. `unauthorized`: the desktop rejected the token (the caller clears the
 * pairing). `paused`: reachable but the service is switched off.
 */
export type ConnectionPhase =
  | "idle"
  | "connecting"
  | "connected"
  | "reconnecting"
  | "unauthorized"
  | "paused";

export type Slice =
  | "status"
  | "inputs"
  | "outputs"
  | "activeInput"
  | "activeOutput"
  | "volume"
  | "eq"
  | "sampleRate"
  | "airplayMode"
  | "cd";

/** A failed required slice is reported; one of these failing is "partly refreshed". */
export const REQUIRED_SLICES: readonly Slice[] = [
  "status",
  "inputs",
  "outputs",
  "activeInput",
  "activeOutput",
  "volume",
  "eq",
  "sampleRate",
];
/** Older desktops lack these endpoints; a failure here is not an error. */
export const OPTIONAL_SLICES: readonly Slice[] = ["airplayMode", "cd"];
export const ALL_SLICES: readonly Slice[] = [...REQUIRED_SLICES, ...OPTIONAL_SLICES];

export type RefreshReason =
  | "connect"
  | "foreground"
  | "poll"
  | "ws-open"
  | "ws-lost"
  | "event"
  | "control"
  | "manual";

export type Snapshot = {
  status: StatusResponse | null;
  inputs: string[];
  outputs: OutputInfo[];
  activeInput: string;
  activeOutput: ActiveOutput | null;
  volume: number;
  gains: EqGains;
  sampleRate: number;
  outputSampleRate: number;
  inputRates: number[];
  outputRates: number[];
  airplayMode: AirPlayMode | null;
  cd: CdStatus;
};

export const EMPTY_CD: CdStatus = {
  present: false,
  playing: false,
  track: 0,
  track_count: 0,
  tracks: [],
  position_ms: 0,
  duration_ms: 0,
};

export const EMPTY_SNAPSHOT: Snapshot = {
  status: null,
  inputs: [],
  outputs: [],
  activeInput: "",
  activeOutput: null,
  volume: 50,
  gains: [0, 0, 0, 0, 0],
  sampleRate: 44_100,
  outputSampleRate: 44_100,
  inputRates: [44_100, 48_000],
  outputRates: [44_100, 48_000],
  airplayMode: null,
  cd: EMPTY_CD,
};

export const PARTIAL_REFRESH_ERROR =
  "Desktop connected. Some controls could not refresh. Try again.";
export const OFFLINE_ERROR =
  "Reconnecting to your desktop… Your pairing is saved. Check that both devices are on the same Wi-Fi.";

export type DesktopConnectionOptions = {
  base: string | null;
  token: string | null;
  /** App in the foreground: the socket and polling run only while true. */
  active: boolean;
  /** The desktop rejected the token; the message is ready to show. */
  onUnauthorized?: (message: string) => void;
  /** Injected for tests. */
  WebSocket?: WebSocketConstructor;
  pollMs?: number;
  coalesceMs?: number;
  stallMs?: number;
};

export type DesktopConnection = Snapshot & {
  phase: ConnectionPhase;
  wsState: ConnectionState;
  /** A person-initiated refresh is running. */
  refreshing: boolean;
  /** Outcome of the last refresh, when something failed. */
  error: string | null;
  /** Coalesces bursts into one fetch of the union of slices. */
  scheduleRefresh(reason: RefreshReason, slices?: readonly Slice[]): void;
  /** Runs now (after any in-flight refresh) and resolves when done. */
  refreshNow(slices?: readonly Slice[]): Promise<void>;
  /** Local update after a control call returned, or an optimistic edit. */
  patch(update: Partial<Snapshot>): void;
};

export const DEFAULT_POLL_MS = 30_000;
export const DEFAULT_COALESCE_MS = 150;
/** React Native cannot observe WebSocket pings; an idle desktop only sends events. */
export const DEFAULT_STALL_MS = 60_000;

type SliceValue = {
  status: StatusResponse;
  inputs: string[];
  outputs: OutputInfo[];
  activeInput: ActiveInputResponse;
  activeOutput: ActiveOutput | null;
  volume: number;
  eq: EqGains;
  sampleRate: SampleRateResponse;
  airplayMode: AirPlayMode;
  cd: CdStatus;
};

type SliceResults = { [K in Slice]?: PromiseSettledResult<SliceValue[K]> };

const readers: { [K in Slice]: (base: string, token: string) => Promise<SliceValue[K]> } = {
  status: (base) => fetchStatus(base),
  inputs: (base, token) => listInputs(base, token),
  outputs: (base, token) => listOutputs(base, token),
  activeInput: (base, token) => getActiveInput(base, token),
  activeOutput: (base, token) => getActiveOutput(base, token),
  volume: (base, token) => getVolume(base, token),
  eq: (base, token) => getEq(base, token),
  sampleRate: (base, token) => getSampleRate(base, token),
  airplayMode: (base, token) => getAirplayMode(base, token),
  cd: (base, token) => getCd(base, token),
};

async function readSlices(base: string, token: string, slices: Slice[]): Promise<SliceResults> {
  const settled = await Promise.allSettled(slices.map((slice) => readers[slice](base, token)));
  const results: SliceResults = {};
  slices.forEach((slice, index) => {
    (results as Record<Slice, PromiseSettledResult<unknown>>)[slice] = settled[index];
  });
  return results;
}

function fulfilled<K extends Slice>(results: SliceResults, slice: K): SliceValue[K] | undefined {
  const result = results[slice];
  return result?.status === "fulfilled" ? (result.value as SliceValue[K]) : undefined;
}

function rejection(results: SliceResults, slice: Slice): unknown {
  const result = results[slice];
  return result?.status === "rejected" ? result.reason : undefined;
}

/** Folds fulfilled slices into the snapshot; rejected ones leave it untouched. */
export function mergeResults(previous: Snapshot, results: SliceResults): Snapshot {
  const next = { ...previous };
  const status = fulfilled(results, "status");
  if (status) next.status = status;
  const inputs = fulfilled(results, "inputs");
  if (inputs) next.inputs = inputs;
  const outputs = fulfilled(results, "outputs");
  if (outputs) next.outputs = outputs;
  const activeInput = fulfilled(results, "activeInput");
  if (activeInput) next.activeInput = activeInput.name ?? "";
  if (results.activeOutput?.status === "fulfilled") next.activeOutput = results.activeOutput.value;
  const volume = fulfilled(results, "volume");
  if (volume !== undefined) next.volume = volume;
  const eq = fulfilled(results, "eq");
  if (eq) next.gains = eq;
  const sampleRate = fulfilled(results, "sampleRate");
  if (sampleRate) {
    next.sampleRate = sampleRate.input.sample_rate_hz;
    next.outputSampleRate = sampleRate.output.sample_rate_hz;
    if (sampleRate.input.supported_hz.length) next.inputRates = sampleRate.input.supported_hz;
    if (sampleRate.output.supported_hz.length) next.outputRates = sampleRate.output.supported_hz;
  }
  const airplayMode = fulfilled(results, "airplayMode");
  if (airplayMode) next.airplayMode = airplayMode;
  const cd = fulfilled(results, "cd");
  if (cd) next.cd = cd;
  return next;
}

export type RefreshOutcome =
  | { kind: "unauthorized"; error: unknown }
  | { kind: "settled"; phase: "connected" | "reconnecting" | "paused"; error: string | null }
  /** Only optional slices were asked for and none reached the desktop: keep the phase. */
  | { kind: "inconclusive" };

/** The desktop answered, even if with an error status. */
function reached(results: SliceResults, slice: Slice): boolean {
  const result = results[slice];
  if (!result) return false;
  return result.status === "fulfilled" || result.reason instanceof HttpError;
}

/**
 * Decides the phase from one refresh. Authentication loss beats everything
 * else; a paused service beats failures; reachability is judged on the
 * required slices when any were requested, so a missing optional endpoint on
 * an older desktop never reads as an outage.
 */
export function judgeResults(results: SliceResults, requested: readonly Slice[]): RefreshOutcome {
  for (const slice of requested) {
    const reason = rejection(results, slice);
    if (isUnauthorized(reason)) return { kind: "unauthorized", error: reason };
  }
  const status = fulfilled(results, "status");
  const paused =
    status?.service_enabled === false ||
    requested.some((slice) => isPaused(rejection(results, slice)));
  if (paused) return { kind: "settled", phase: "paused", error: null };
  const required = requested.filter((slice) => REQUIRED_SLICES.includes(slice));
  const considered = required.length > 0 ? required : requested;
  const reachable = considered.some((slice) => reached(results, slice));
  if (!reachable) {
    return required.length > 0
      ? { kind: "settled", phase: "reconnecting", error: OFFLINE_ERROR }
      : { kind: "inconclusive" };
  }
  const failedRequired = required.some((slice) => results[slice]?.status === "rejected");
  return {
    kind: "settled",
    phase: "connected",
    error: failedRequired ? PARTIAL_REFRESH_ERROR : null,
  };
}

/**
 * One desktop pairing's live state: a coalescing refresh scheduler over the
 * HTTP reads, the event socket that applies payloads directly, and a slow
 * poll as the fallback. The phase is decided by HTTP results; socket state is
 * exposed separately so it never flaps the UI on its own.
 */
export function useDesktopConnection({
  base,
  token,
  active,
  onUnauthorized,
  WebSocket,
  pollMs = DEFAULT_POLL_MS,
  coalesceMs = DEFAULT_COALESCE_MS,
  stallMs = DEFAULT_STALL_MS,
}: DesktopConnectionOptions): DesktopConnection {
  const [snapshot, setSnapshot] = useState<Snapshot>(EMPTY_SNAPSHOT);
  const [phase, setPhase] = useState<ConnectionPhase>("idle");
  const [wsState, setWsState] = useState<ConnectionState>("closed");
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const snapshotRef = useRef(snapshot);
  snapshotRef.current = snapshot;
  const credentials = useRef<{ base: string; token: string } | null>(null);
  credentials.current = base && token ? { base, token } : null;
  const onUnauthorizedRef = useRef(onUnauthorized);
  onUnauthorizedRef.current = onUnauthorized;

  const generation = useRef(0);
  const inFlight = useRef<Promise<void> | null>(null);
  const pendingSlices = useRef(new Set<Slice>());
  const waiters = useRef<Array<() => void>>([]);
  const coalesceTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const manualCount = useRef(0);
  const mounted = useRef(true);

  const clearCoalesce = useCallback(() => {
    if (coalesceTimer.current) clearTimeout(coalesceTimer.current);
    coalesceTimer.current = null;
  }, []);

  const run = useCallback(async (slices: Slice[], startedIn: number) => {
    const creds = credentials.current;
    if (!creds) return;
    const results = await readSlices(creds.base, creds.token, slices);
    if (!mounted.current || startedIn !== generation.current) return;
    const outcome = judgeResults(results, slices);
    if (outcome.kind === "unauthorized") {
      setPhase("unauthorized");
      const message = friendlyError(outcome.error, "refresh the mixer");
      setError(message);
      onUnauthorizedRef.current?.(message);
      return;
    }
    setSnapshot((previous) => mergeResults(previous, results));
    if (outcome.kind === "inconclusive") return;
    setPhase(outcome.phase);
    setError(outcome.error);
  }, []);

  const flush = useCallback(() => {
    clearCoalesce();
    if (inFlight.current) return;
    if (pendingSlices.current.size === 0) return;
    const slices = [...pendingSlices.current];
    pendingSlices.current.clear();
    const settle = waiters.current;
    waiters.current = [];
    const startedIn = generation.current;
    const promise = run(slices, startedIn)
      .catch(() => {
        /* readSlices settles every read; nothing else throws. */
      })
      .finally(() => {
        if (inFlight.current === promise) inFlight.current = null;
        for (const resolve of settle) resolve();
        if (pendingSlices.current.size > 0) flush();
      });
    inFlight.current = promise;
  }, [run, clearCoalesce]);

  const scheduleRefresh = useCallback(
    (reason: RefreshReason, slices: readonly Slice[] = ALL_SLICES) => {
      if (!credentials.current) return;
      for (const slice of slices) pendingSlices.current.add(slice);
      const immediate = reason === "connect" || reason === "foreground" || reason === "manual";
      if (immediate) {
        flush();
        return;
      }
      if (!coalesceTimer.current) coalesceTimer.current = setTimeout(flush, coalesceMs);
    },
    [flush, coalesceMs],
  );

  const refreshNow = useCallback(
    (slices: readonly Slice[] = ALL_SLICES): Promise<void> => {
      if (!credentials.current) return Promise.resolve();
      manualCount.current += 1;
      setRefreshing(true);
      const done = new Promise<void>((resolve) => waiters.current.push(resolve));
      scheduleRefresh("manual", slices);
      return done.finally(() => {
        manualCount.current -= 1;
        if (mounted.current && manualCount.current === 0) setRefreshing(false);
      });
    },
    [scheduleRefresh],
  );

  const patch = useCallback((update: Partial<Snapshot>) => {
    setSnapshot((previous) => ({ ...previous, ...update }));
  }, []);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      clearCoalesce();
    };
  }, [clearCoalesce]);

  // A new pairing (or none) starts from a clean slate and orphans in-flight reads.
  useEffect(() => {
    generation.current += 1;
    clearCoalesce();
    pendingSlices.current.clear();
    inFlight.current = null;
    const settle = waiters.current;
    waiters.current = [];
    for (const resolve of settle) resolve();
    setSnapshot(EMPTY_SNAPSHOT);
    setError(null);
    setPhase(base && token ? "connecting" : "idle");
  }, [base, token, clearCoalesce]);

  const onEvent = useCallback(
    (event: WsEvent) => {
      switch (event.type) {
        case "LevelMeter":
          return;
        case "ServiceStateChanged":
          setSnapshot((previous) => ({
            ...previous,
            status: previous.status ? { ...previous.status, service_enabled: event.enabled } : null,
          }));
          if (event.enabled) {
            setPhase((current) => (current === "paused" ? "connected" : current));
            scheduleRefresh("event");
          } else {
            setPhase("paused");
            setError(null);
          }
          return;
        case "CdStateChanged": {
          const { type: _type, ...fields } = event;
          const previous = snapshotRef.current.cd;
          setSnapshot((current) => ({ ...current, cd: { ...current.cd, ...fields } }));
          if (fields.present !== previous.present || fields.track_count !== previous.track_count) {
            scheduleRefresh("event", ["cd"]);
          }
          return;
        }
        case "OutputStateChanged": {
          if (!event.active) {
            setSnapshot((current) => ({ ...current, activeOutput: null }));
            return;
          }
          const match = snapshotRef.current.outputs.find(
            (output) => output.transport === event.transport && output.name === event.device_name,
          );
          setSnapshot((current) => ({
            ...current,
            activeOutput: {
              transport: event.transport,
              device_name: event.device_name,
              device_id: match?.id ?? current.activeOutput?.device_id ?? "",
            },
          }));
          scheduleRefresh("event", match ? ["volume"] : ["activeOutput", "volume"]);
          return;
        }
        case "DeviceJoined":
        case "DeviceLeft":
          scheduleRefresh("event", ["outputs"]);
          return;
        default:
          return;
      }
    },
    [scheduleRefresh],
  );

  useEffect(() => {
    if (!base || !token || !active) {
      setWsState("closed");
      return;
    }
    scheduleRefresh("connect");
    let opened = false;
    const subscription = subscribeEvents(
      base,
      token,
      {
        onEvent,
        onStateChange: (state) => {
          if (!mounted.current) return;
          setWsState(state);
          if (state === "open") {
            // The connect refresh already covers the first open.
            if (opened) scheduleRefresh("ws-open");
            opened = true;
          } else if (state === "reconnecting") {
            scheduleRefresh("ws-lost");
          }
        },
      },
      { WebSocket, stallMs },
    );
    const poll = setInterval(() => scheduleRefresh("poll"), pollMs);
    return () => {
      clearInterval(poll);
      subscription.close();
      clearCoalesce();
      pendingSlices.current.clear();
    };
  }, [base, token, active, onEvent, scheduleRefresh, WebSocket, pollMs, stallMs, clearCoalesce]);

  return {
    ...snapshot,
    phase,
    wsState,
    refreshing,
    error,
    scheduleRefresh,
    refreshNow,
    patch,
  };
}
