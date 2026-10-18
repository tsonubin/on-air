import type {
  ActiveOutputView,
  AirPlayMode,
  AudioQualityResponse,
  CdStatus,
  EqGains,
  OutputInfo,
  OutputSyncStatus,
  StatusResponse,
  SyncGroupView,
  WsEvent,
} from "@on-air/api-types";
import * as client from "@on-air/control-client";
import {
  ALL_SLICES,
  type ConnectionState,
  type CoreSnapshot,
  createRefreshQueue,
  createStaleGuard,
  EMPTY_SNAPSHOT,
  judgeResults,
  mergeResults,
  type RefreshQueue,
  readSlices,
  reduceEvent,
  type Slice,
  type StereoLevels,
  sliceReaders,
  subscribeEvents,
  type WebSocketConstructor,
} from "@on-air/control-client";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { friendlyError } from "@/errors";

export { ALL_SLICES, OPTIONAL_SLICES, REQUIRED_SLICES, type Slice } from "@on-air/control-client";

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

export type RefreshReason =
  | "connect"
  | "foreground"
  | "poll"
  | "ws-open"
  | "ws-lost"
  | "event"
  | "control"
  | "manual";

/** The snapshot as the screens read it: no nulls, sentinel defaults until the first refresh. */
export type Snapshot = {
  status: StatusResponse | null;
  inputs: string[];
  outputs: OutputInfo[];
  activeInput: string;
  /** `state` is absent on older desktops; treat that as live. */
  activeOutput: ActiveOutputView | null;
  volume: number;
  gains: EqGains;
  sampleRate: number;
  outputSampleRate: number;
  inputRates: number[];
  outputRates: number[];
  /** DSP quality profiles; `null` on a desktop too old to report them. */
  audioQuality: AudioQualityResponse | null;
  /** Per-output latency trim and sync state; empty on an older desktop. */
  sync: OutputSyncStatus[];
  /** Sync groups, empty when the desktop predates them. */
  syncGroups: SyncGroupView[];
  airplayMode: AirPlayMode | null;
  cd: CdStatus;
};

const EMPTY_CD: CdStatus = {
  present: false,
  playing: false,
  track: 0,
  track_count: 0,
  tracks: [],
  position_ms: 0,
  duration_ms: 0,
};
const DEFAULT_RATES = [44_100, 48_000];

function toView(s: CoreSnapshot): Snapshot {
  return {
    status: s.status,
    inputs: s.inputs ?? [],
    outputs: s.outputs ?? [],
    activeInput: s.activeInput ?? "",
    activeOutput: s.activeOutput,
    volume: s.volume ?? 50,
    gains: s.gains ?? [0, 0, 0, 0, 0],
    sampleRate: s.sampleRate?.input.sample_rate_hz ?? 44_100,
    outputSampleRate: s.sampleRate?.output.sample_rate_hz ?? 44_100,
    inputRates: s.sampleRate?.input.supported_hz.length
      ? s.sampleRate.input.supported_hz
      : DEFAULT_RATES,
    outputRates: s.sampleRate?.output.supported_hz.length
      ? s.sampleRate.output.supported_hz
      : DEFAULT_RATES,
    audioQuality: s.audioQuality,
    sync: s.sync?.outputs ?? [],
    syncGroups: s.sync?.groups ?? [],
    airplayMode: s.airplayMode,
    cd: s.cd ?? EMPTY_CD,
  };
}

/** The slice each view field is read from, so a local patch stamps the right one. */
const SLICE_OF: { [K in keyof Snapshot]: Slice } = {
  status: "status",
  inputs: "inputs",
  outputs: "outputs",
  activeInput: "activeInput",
  activeOutput: "activeOutput",
  volume: "volume",
  gains: "gains",
  sampleRate: "sampleRate",
  outputSampleRate: "sampleRate",
  inputRates: "sampleRate",
  outputRates: "sampleRate",
  audioQuality: "audioQuality",
  sync: "sync",
  syncGroups: "sync",
  airplayMode: "airplayMode",
  cd: "cd",
};

/** Writes a view-shaped patch back into the nullable snapshot. */
function applyPatch(prev: CoreSnapshot, update: Partial<Snapshot>): CoreSnapshot {
  const next: CoreSnapshot = { ...prev };
  if (update.status !== undefined) next.status = update.status;
  if (update.inputs !== undefined) next.inputs = update.inputs;
  if (update.outputs !== undefined) next.outputs = update.outputs;
  if (update.activeInput !== undefined) next.activeInput = update.activeInput || null;
  if (update.activeOutput !== undefined) next.activeOutput = update.activeOutput;
  if (update.volume !== undefined) next.volume = update.volume;
  if (update.gains !== undefined) next.gains = update.gains;
  if (update.audioQuality !== undefined) next.audioQuality = update.audioQuality;
  // Rows and groups share one wire response; a patch of either keeps the other.
  if (update.sync !== undefined || update.syncGroups !== undefined) {
    next.sync = {
      outputs: update.sync ?? prev.sync?.outputs ?? [],
      groups: update.syncGroups ?? prev.sync?.groups,
    };
  }
  if (update.airplayMode !== undefined) next.airplayMode = update.airplayMode;
  if (update.cd !== undefined) next.cd = update.cd;
  const rates = prev.sampleRate;
  if (rates) {
    const input = {
      ...rates.input,
      sample_rate_hz: update.sampleRate ?? rates.input.sample_rate_hz,
      supported_hz: update.inputRates ?? rates.input.supported_hz,
    };
    const output = {
      ...rates.output,
      sample_rate_hz: update.outputSampleRate ?? rates.output.sample_rate_hz,
      supported_hz: update.outputRates ?? rates.output.supported_hz,
    };
    next.sampleRate = { ...rates, sample_rate_hz: input.sample_rate_hz, input, output };
  }
  return next;
}

export const PARTIAL_REFRESH_ERROR =
  "Desktop connected. Some controls could not refresh. Try again.";
export const OFFLINE_ERROR =
  "Reconnecting to your desktop… Your pairing is saved. Check that both devices are on the same Wi-Fi.";
export const INPUT_STOPPED_NOTICE =
  "The source stopped unexpectedly on the desktop. Choose it again to restart.";

export type DesktopConnectionOptions = {
  base: string | null;
  token: string | null;
  /** App in the foreground: the socket and polling run only while true. */
  active: boolean;
  /** The desktop rejected the token; the message is ready to show. */
  onUnauthorized?: (message: string) => void;
  /** Something stopped on the desktop on its own; the message is ready to show. */
  onNotice?: (message: string) => void;
  /** Injected for tests. */
  WebSocket?: WebSocketConstructor;
  pollMs?: number;
  coalesceMs?: number;
  stallMs?: number;
};

export type DesktopConnection = Snapshot & {
  phase: ConnectionPhase;
  wsState: ConnectionState;
  /** The latest stereo level reading, or `null` while none is arriving. */
  levels: StereoLevels | null;
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
/**
 * React Native cannot observe WebSocket pings, so the desktop sends a
 * `Heartbeat` text frame every 10 s; three missed ones mean a dead socket.
 */
export const DEFAULT_STALL_MS = 30_000;
/** Levels arrive ~43/s; the meter re-renders at most at this interval. */
export const METER_INTERVAL_MS = 50;

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
  onNotice,
  WebSocket,
  pollMs = DEFAULT_POLL_MS,
  coalesceMs = DEFAULT_COALESCE_MS,
  stallMs = DEFAULT_STALL_MS,
}: DesktopConnectionOptions): DesktopConnection {
  const [snapshot, setSnapshot] = useState<CoreSnapshot>(EMPTY_SNAPSHOT);
  const [phase, setPhase] = useState<ConnectionPhase>("idle");
  const [wsState, setWsState] = useState<ConnectionState>("closed");
  const [levels, setLevels] = useState<StereoLevels | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const lastLevelsAt = useRef(0);
  const credentials = useRef<{ base: string; token: string } | null>(null);
  credentials.current = base && token ? { base, token } : null;
  const onUnauthorizedRef = useRef(onUnauthorized);
  onUnauthorizedRef.current = onUnauthorized;
  const onNoticeRef = useRef(onNotice);
  onNoticeRef.current = onNotice;

  // The ref is the source of truth so events and merges compose in order
  // even when several land before React renders; state mirrors it.
  const snapshotRef = useRef(snapshot);
  const update = useCallback((f: (prev: CoreSnapshot) => CoreSnapshot) => {
    const next = f(snapshotRef.current);
    if (next === snapshotRef.current) return;
    snapshotRef.current = next;
    setSnapshot(next);
  }, []);
  const guard = useRef(createStaleGuard<Slice>()).current;

  // A new pairing orphans reads started for the old one.
  const generation = useRef(0);
  const manualCount = useRef(0);
  const mounted = useRef(true);

  const run = useCallback(
    async (slices: Slice[], startedIn: number) => {
      const creds = credentials.current;
      if (!creds) return;
      const since = guard.now();
      const results = await readSlices(sliceReaders(client, creds.base, creds.token), slices);
      if (!mounted.current || startedIn !== generation.current) return;
      const stale = (slice: Slice) => guard.isStale(slice, since);
      const outcome = judgeResults(results, slices);
      if (outcome.kind === "unauthorized") {
        setPhase("unauthorized");
        const message = friendlyError(outcome.error, "refresh the mixer");
        setError(message);
        onUnauthorizedRef.current?.(message);
        return;
      }
      update((prev) => mergeResults(prev, results, stale));
      if (outcome.kind === "inconclusive") return;
      // A ServiceStateChanged landed meanwhile and already set the phase.
      if (slices.includes("status") && stale("status")) return;
      setPhase(outcome.phase);
      setError(
        outcome.phase === "reconnecting"
          ? OFFLINE_ERROR
          : outcome.partial
            ? PARTIAL_REFRESH_ERROR
            : null,
      );
    },
    [guard, update],
  );
  const runRef = useRef(run);
  runRef.current = run;
  const queue = useRef<RefreshQueue<Slice> | null>(null);
  if (queue.current === null) {
    queue.current = createRefreshQueue<Slice>(
      (slices) => runRef.current(slices, generation.current),
      coalesceMs,
    );
  }

  const scheduleRefresh = useCallback(
    (reason: RefreshReason, slices: readonly Slice[] = ALL_SLICES) => {
      if (!credentials.current) return;
      const immediate = reason === "connect" || reason === "foreground" || reason === "manual";
      queue.current?.schedule(slices, immediate);
    },
    [],
  );

  const refreshNow = useCallback((slices: readonly Slice[] = ALL_SLICES): Promise<void> => {
    if (!credentials.current || !queue.current) return Promise.resolve();
    manualCount.current += 1;
    setRefreshing(true);
    return queue.current.refreshNow(slices).finally(() => {
      manualCount.current -= 1;
      if (mounted.current && manualCount.current === 0) setRefreshing(false);
    });
  }, []);

  const patch = useCallback(
    (patched: Partial<Snapshot>) => {
      guard.touch(...(Object.keys(patched) as (keyof Snapshot)[]).map((key) => SLICE_OF[key]));
      update((prev) => applyPatch(prev, patched));
    },
    [guard, update],
  );

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      queue.current?.clear();
    };
  }, []);

  // A new pairing (or none) starts from a clean slate and orphans in-flight reads.
  useEffect(() => {
    generation.current += 1;
    guard.reset();
    queue.current?.clear();
    update(() => EMPTY_SNAPSHOT);
    setLevels(null);
    setError(null);
    setPhase(base && token ? "connecting" : "idle");
  }, [base, token, guard, update]);

  const onEvent = useCallback(
    (event: WsEvent) => {
      const reduced = reduceEvent(snapshotRef.current, event);
      if (reduced.meter) {
        // Levels never touch the snapshot; throttle them to the meter's
        // frame rate and drop the rest.
        const now = Date.now();
        if (now - lastLevelsAt.current >= METER_INTERVAL_MS) {
          lastLevelsAt.current = now;
          setLevels(reduced.meter);
        }
        return;
      }
      guard.touch(...reduced.touched);
      update(() => reduced.next);
      if (reduced.inputStopped) onNoticeRef.current?.(INPUT_STOPPED_NOTICE);
      if (reduced.serviceEnabled === true) {
        setPhase((current) => (current === "paused" ? "connected" : current));
      } else if (reduced.serviceEnabled === false) {
        setPhase("paused");
        setError(null);
      }
      if (reduced.refresh.length > 0) scheduleRefresh("event", reduced.refresh);
    },
    [guard, scheduleRefresh, update],
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
      queue.current?.clear();
    };
  }, [base, token, active, onEvent, scheduleRefresh, WebSocket, pollMs, stallMs]);

  const view = useMemo(() => toView(snapshot), [snapshot]);

  return {
    ...view,
    phase,
    wsState,
    levels,
    refreshing,
    error,
    scheduleRefresh,
    refreshNow,
    patch,
  };
}
