import type {
  ActiveOutputView,
  AirPlayMode,
  CdStatus,
  EqGains,
  OutputInfo,
  SampleRateResponse,
  StatusResponse,
  WsEvent,
} from "@on-air/api-types";
import type { ConnectionState, SubscribeOptions } from "@on-air/control-client";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { type ClientLike, liveClient } from "../lib/client";
import { errorCopy, INPUT_STOPPED_COPY, isServicePaused } from "../lib/errorCopy";

/** Everything the UI reads from the core. Each field is `null` until its first fetch lands. */
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
  pin: string | null;
  airplayMode: AirPlayMode | null;
  cd: CdStatus | null;
}

/** Health of the HTTP side, derived from `/api/status` and fetch failures only. */
export type Connection = "connecting" | "ok" | "unreachable" | "paused";

/** Which group of endpoints a refresh covers. */
export type RefreshKind = "devices" | "config" | "all";

/** A failed user action. Only the next action or an explicit dismiss clears it. */
export interface ActionError {
  message: string;
  at: number;
}

export interface UseCoreSnapshotOptions {
  client?: ClientLike;
  /** Full poll interval. */
  pollMs?: number;
  /** Window in which repeated refresh requests are coalesced. */
  debounceMs?: number;
  /** Forwarded to `subscribeEvents`; tests inject a fake socket here. */
  subscribeOptions?: SubscribeOptions;
}

export interface CoreSnapshotHandle {
  snapshot: CoreSnapshot;
  connection: Connection;
  /** State of the event socket; `"reconnecting"` is shown as "live updates off". */
  liveUpdates: ConnectionState;
  actionError: ActionError | null;
  reportError(err: unknown): void;
  clearError(): void;
  /** Debounced: runs at once, then at most once more after the window closes. */
  refresh(kind?: RefreshKind): Promise<void>;
  /** Apply the result of a user action without waiting for a poll. */
  patch: {
    cd(next: CdStatus): void;
    activeOutput(next: ActiveOutputView | null): void;
    activeInput(next: string | null): void;
  };
}

export const DEFAULT_POLL_MS = 15_000;
export const DEFAULT_DEBOUNCE_MS = 300;
/** The core sends a Heartbeat text frame every 10 s; three missed ones mean a dead socket. */
export const EVENT_STALL_MS = 30_000;

const EMPTY: CoreSnapshot = {
  status: null,
  inputs: null,
  activeInput: null,
  outputs: null,
  activeOutput: null,
  volume: null,
  gains: null,
  sampleRate: null,
  pin: null,
  airplayMode: null,
  cd: null,
};

const MISS = Symbol("miss");
type Settled<T> = T | typeof MISS;

type Field = keyof CoreSnapshot;

/**
 * Fields a `DeviceJoined` event does not carry. A debounced devices refresh
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

function mergeKinds(a: RefreshKind, b: RefreshKind): RefreshKind {
  return a === b ? a : "all";
}

export function useCoreSnapshot(options: UseCoreSnapshotOptions = {}): CoreSnapshotHandle {
  const {
    client = liveClient,
    pollMs = DEFAULT_POLL_MS,
    debounceMs = DEFAULT_DEBOUNCE_MS,
    subscribeOptions,
  } = options;

  const [snapshot, setSnapshot] = useState<CoreSnapshot>(EMPTY);
  const [connection, setConnection] = useState<Connection>("connecting");
  const [liveUpdates, setLiveUpdates] = useState<ConnectionState>("connecting");
  const [actionError, setActionError] = useState<ActionError | null>(null);

  // Mirrors of state the fetchers need without re-creating callbacks.
  const snapshotRef = useRef(snapshot);
  snapshotRef.current = snapshot;
  const connectionRef = useRef(connection);
  connectionRef.current = connection;
  const clientRef = useRef(client);
  clientRef.current = client;

  const patchSnapshot = useCallback((partial: Partial<CoreSnapshot>) => {
    setSnapshot((prev) => ({ ...prev, ...partial }));
  }, []);

  // Every event or local patch bumps `generation` and stamps the fields it
  // set. A fetch drops any field stamped after it started, so a poll that
  // left before an event cannot land after it and restore stale state.
  const generation = useRef(0);
  const touchedAt = useRef<Partial<Record<Field, number>>>({});
  const touch = useCallback((...fields: Field[]) => {
    generation.current += 1;
    for (const field of fields) touchedAt.current[field] = generation.current;
  }, []);

  // ---------------------------------------------------------------------
  // Fetching
  // ---------------------------------------------------------------------

  /** One fetch group; failures are swallowed per endpoint and reported via `connection`. */
  const fetchNow = useCallback(
    async (kind: RefreshKind): Promise<void> => {
      const api = clientRef.current;
      const startedAt = generation.current;
      const stale = (field: Field) => (touchedAt.current[field] ?? 0) > startedAt;
      let pausedByCode = false;
      const settle = async <T>(p: Promise<T>): Promise<Settled<T>> => {
        try {
          return await p;
        } catch (err) {
          if (isServicePaused(err)) pausedByCode = true;
          return MISS;
        }
      };

      let status: Settled<StatusResponse>;
      try {
        status = await api.fetchStatus();
      } catch {
        setConnection("unreachable");
        return;
      }
      // A ServiceStateChanged landed meanwhile; it set the connection itself
      // and, when enabling, scheduled its own refresh.
      if (stale("status")) return;
      patchSnapshot({ status });
      const paused = status.service_enabled === false;
      if (paused) {
        // `Paired` answers 503 to everything else while paused; do not spam it.
        setConnection("paused");
        return;
      }

      const wantDevices = kind !== "config";
      const wantConfig = kind !== "devices";
      const skip = Promise.resolve(MISS);
      const [inputs, activeInput, outputs, activeOutput, cd, volume, gains, sampleRate, pin, mode] =
        await Promise.all([
          wantDevices ? settle(api.listInputs()) : skip,
          wantDevices ? settle(api.getActiveInput()) : skip,
          wantDevices ? settle(api.listOutputs()) : skip,
          wantDevices ? settle(api.getActiveOutput()) : skip,
          wantDevices ? settle(api.getCd()) : skip,
          wantConfig ? settle(api.getVolume()) : skip,
          wantConfig ? settle(api.getEq()) : skip,
          wantConfig ? settle(api.getSampleRate()) : skip,
          wantConfig ? settle(api.getPairingPin()) : skip,
          wantConfig ? settle(api.getAirplayMode()) : skip,
        ]);

      const next: Partial<CoreSnapshot> = {};
      if (inputs !== MISS) next.inputs = inputs;
      if (activeInput !== MISS) next.activeInput = activeInput.name;
      if (outputs !== MISS) next.outputs = outputs;
      if (activeOutput !== MISS) next.activeOutput = activeOutput;
      if (cd !== MISS) next.cd = cd;
      if (volume !== MISS) next.volume = volume;
      if (gains !== MISS) next.gains = gains;
      if (sampleRate !== MISS) next.sampleRate = sampleRate;
      if (pin !== MISS) next.pin = pin;
      if (mode !== MISS) next.airplayMode = mode;
      for (const field of Object.keys(next) as Field[]) {
        if (stale(field)) delete next[field];
      }
      patchSnapshot(next);
      setConnection(pausedByCode ? "paused" : "ok");
    },
    [patchSnapshot],
  );

  // Debounce (leading edge plus one trailing run) on top of in-flight
  // serialisation, so a burst of socket events costs at most two rounds.
  const inFlight = useRef<Promise<void> | null>(null);
  const windowTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const trailing = useRef<{ kind: RefreshKind; promise: Promise<void>; run: () => void } | null>(
    null,
  );

  const runSerialised = useCallback(
    (kind: RefreshKind): Promise<void> => {
      const previous = inFlight.current ?? Promise.resolve();
      const run = previous.then(() => fetchNow(kind));
      inFlight.current = run;
      void run.finally(() => {
        if (inFlight.current === run) inFlight.current = null;
      });
      return run;
    },
    [fetchNow],
  );

  const refresh = useCallback(
    (kind: RefreshKind = "all"): Promise<void> => {
      if (windowTimer.current === null) {
        windowTimer.current = setTimeout(() => {
          windowTimer.current = null;
          const queued = trailing.current;
          trailing.current = null;
          queued?.run();
        }, debounceMs);
        return runSerialised(kind);
      }
      if (trailing.current) {
        trailing.current.kind = mergeKinds(trailing.current.kind, kind);
        return trailing.current.promise;
      }
      let resolve!: () => void;
      let reject!: (err: unknown) => void;
      const promise = new Promise<void>((res, rej) => {
        resolve = res;
        reject = rej;
      });
      const entry = {
        kind,
        promise,
        run: () => {
          runSerialised(entry.kind).then(resolve, reject);
        },
      };
      trailing.current = entry;
      return promise;
    },
    [debounceMs, runSerialised],
  );

  const refreshRef = useRef(refresh);
  refreshRef.current = refresh;

  // Initial load, poll, and focus / visibility triggers.
  useEffect(() => {
    const refreshIfVisible = () => {
      if (!document.hidden) void refreshRef.current("all").catch(() => undefined);
    };
    refreshIfVisible();
    const id = setInterval(refreshIfVisible, pollMs);
    document.addEventListener("visibilitychange", refreshIfVisible);
    window.addEventListener("focus", refreshIfVisible);
    return () => {
      clearInterval(id);
      document.removeEventListener("visibilitychange", refreshIfVisible);
      window.removeEventListener("focus", refreshIfVisible);
      if (windowTimer.current !== null) clearTimeout(windowTimer.current);
      windowTimer.current = null;
      trailing.current = null;
    };
  }, [pollMs]);

  // ---------------------------------------------------------------------
  // Live events
  // ---------------------------------------------------------------------

  const applyEvent = useCallback(
    (ev: WsEvent) => {
      switch (ev.type) {
        case "Heartbeat":
        case "LevelMeter":
          return;
        case "CdStateChanged": {
          const { type: _type, ...fields } = ev;
          const wasPresent = snapshotRef.current.cd?.present ?? false;
          touch("cd");
          setSnapshot((prev) => ({
            ...prev,
            cd: { ...(prev.cd ?? { tracks: [] }), ...fields },
          }));
          // A disc arriving or leaving adds or removes the "Audio CD" input,
          // and autoplay may switch to it.
          if (ev.present !== wasPresent) void refreshRef.current("devices").catch(() => undefined);
          return;
        }
        case "InputStateChanged":
          touch("activeInput");
          setSnapshot((prev) => {
            if (ev.active) return { ...prev, activeInput: ev.name };
            return ev.name === null || prev.activeInput === ev.name
              ? { ...prev, activeInput: null }
              : prev;
          });
          if (ev.error) setActionError({ message: INPUT_STOPPED_COPY, at: Date.now() });
          void refreshRef.current("devices").catch(() => undefined);
          return;
        case "ServiceStateChanged":
          touch("status");
          setSnapshot((prev) =>
            prev.status
              ? { ...prev, status: { ...prev.status, service_enabled: ev.enabled } }
              : prev,
          );
          if (ev.enabled) {
            setConnection("ok");
            void refreshRef.current("all").catch(() => undefined);
          } else {
            setConnection("paused");
          }
          return;
        case "OutputStateChanged":
          touch("activeOutput");
          setSnapshot((prev) => {
            if (!ev.active) {
              return prev.activeOutput?.transport === ev.transport
                ? { ...prev, activeOutput: null }
                : prev;
            }
            // The event names the device but does not carry its id; the
            // catalog usually has it, otherwise a devices refresh fills it.
            const match = prev.outputs?.find(
              (o) => o.transport === ev.transport && o.name === ev.device_name,
            );
            if (!match) void refreshRef.current("devices").catch(() => undefined);
            return {
              ...prev,
              activeOutput: {
                transport: ev.transport,
                device_name: ev.device_name,
                device_id: match?.id ?? "",
                // The core only announces an active output once it is live.
                state: "live",
              },
            };
          });
          return;
        case "DeviceJoined":
          touch("outputs");
          setSnapshot((prev) => {
            const outputs = prev.outputs ?? [];
            if (outputs.some((o) => o.transport === ev.transport && o.id === ev.id)) return prev;
            return { ...prev, outputs: [...outputs, provisionalOutput(ev)] };
          });
          void refreshRef.current("devices").catch(() => undefined);
          return;
        case "DeviceLeft":
          touch("outputs");
          setSnapshot((prev) => ({
            ...prev,
            outputs:
              prev.outputs?.filter((o) => !(o.transport === ev.transport && o.id === ev.id)) ??
              null,
          }));
          return;
      }
    },
    [touch],
  );

  // Re-subscribe when the HTTP side recovers: a socket left half-open by a
  // sleep/wake cycle looks "open" to the page until the OS notices.
  const [socketGeneration, setSocketGeneration] = useState(0);
  const previousConnection = useRef<Connection>("connecting");
  useEffect(() => {
    if (previousConnection.current === "unreachable" && connection === "ok") {
      setSocketGeneration((g) => g + 1);
    }
    previousConnection.current = connection;
  }, [connection]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: socketGeneration is the re-subscribe trigger.
  useEffect(() => {
    let opened = false;
    const subscription = clientRef.current.subscribeEvents(
      {
        onEvent: applyEvent,
        onOpen: () => {
          // Events during the gap are gone; one full refresh catches up.
          if (opened) void refreshRef.current("all").catch(() => undefined);
          opened = true;
        },
        onStateChange: setLiveUpdates,
      },
      {
        // The core sends a `Heartbeat` text frame every 10 s even when idle,
        // so a quiet socket for this long is a dead one.
        stallMs: EVENT_STALL_MS,
        ...subscribeOptions,
      },
    );
    return () => subscription.close();
  }, [applyEvent, subscribeOptions, socketGeneration]);

  // ---------------------------------------------------------------------
  // Action errors and patches
  // ---------------------------------------------------------------------

  const reportError = useCallback((err: unknown) => {
    setActionError({ message: errorCopy(err), at: Date.now() });
  }, []);
  const clearError = useCallback(() => setActionError(null), []);

  const patch = useMemo(
    () => ({
      cd: (next: CdStatus) => {
        touch("cd");
        patchSnapshot({ cd: next });
      },
      activeOutput: (next: ActiveOutputView | null) => {
        touch("activeOutput");
        patchSnapshot({ activeOutput: next });
      },
      activeInput: (next: string | null) => {
        touch("activeInput");
        patchSnapshot({ activeInput: next });
      },
    }),
    [patchSnapshot, touch],
  );

  return {
    snapshot,
    connection,
    liveUpdates,
    actionError,
    reportError,
    clearError,
    refresh,
    patch,
  };
}
