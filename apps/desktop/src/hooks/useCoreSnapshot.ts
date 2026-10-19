import type { ActiveOutputView, CdStatus, WsEvent } from "@on-air/api-types";
import {
  ALL_SLICES,
  CONFIG_SLICES,
  type ConnectionState,
  type CoreSnapshot,
  createRefreshQueue,
  createStaleGuard,
  DEVICE_SLICES,
  EMPTY_SNAPSHOT,
  judgeResults,
  mergeResults,
  type RefreshQueue,
  readSlices,
  reduceEvent,
  type Slice,
  type SliceReaders,
  type SubscribeOptions,
} from "@on-air/control-client";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { type ClientLike, liveClient } from "../lib/client";
import { errorCopy, INPUT_STOPPED_COPY } from "../lib/errorCopy";

export type { CoreSnapshot } from "@on-air/control-client";

/** Health of the HTTP side, derived from `/api/status` and fetch failures only. */
export type Connection = "connecting" | "ok" | "unreachable" | "paused";

/** Which group of endpoints a refresh covers. Status is always included. */
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
  /** Window in which event-triggered refreshes are coalesced. */
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
  /** Runs now (after any in-flight refresh) and resolves when done. */
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

// The pairing PIN is a loopback-only read, so only this window asks for it.
const KIND_SLICES: Record<RefreshKind, readonly Slice[]> = {
  all: [...ALL_SLICES, "pin"],
  devices: ["status", ...DEVICE_SLICES],
  config: ["status", ...CONFIG_SLICES, "pin"],
};

function readersFor(client: ClientLike): SliceReaders {
  return {
    status: () => client.fetchStatus(),
    inputs: () => client.listInputs(),
    activeInput: () => client.getActiveInput(),
    outputs: () => client.listOutputs(),
    activeOutput: () => client.getActiveOutput(),
    volume: () => client.getVolume(),
    gains: () => client.getEq(),
    sampleRate: () => client.getSampleRate(),
    pin: () => client.getPairingPin(),
    airplayMode: () => client.getAirplayMode(),
    cd: () => client.getCd(),
  };
}

export function useCoreSnapshot(options: UseCoreSnapshotOptions = {}): CoreSnapshotHandle {
  const {
    client = liveClient,
    pollMs = DEFAULT_POLL_MS,
    debounceMs = DEFAULT_DEBOUNCE_MS,
    subscribeOptions,
  } = options;

  const [snapshot, setSnapshot] = useState<CoreSnapshot>(EMPTY_SNAPSHOT);
  const [connection, setConnection] = useState<Connection>("connecting");
  const [liveUpdates, setLiveUpdates] = useState<ConnectionState>("connecting");
  const [actionError, setActionError] = useState<ActionError | null>(null);
  const clientRef = useRef(client);
  clientRef.current = client;

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

  const run = useCallback(
    async (slices: Slice[]) => {
      const since = guard.now();
      const results = await readSlices(readersFor(clientRef.current), slices);
      const stale = (slice: Slice) => guard.isStale(slice, since);
      update((prev) => mergeResults(prev, results, stale));
      // A ServiceStateChanged landed meanwhile; it set the connection itself.
      if (slices.includes("status") && stale("status")) return;
      const outcome = judgeResults(results, slices);
      if (outcome.kind === "inconclusive") return;
      if (outcome.kind === "unauthorized" || outcome.phase === "reconnecting") {
        setConnection("unreachable");
      } else {
        setConnection(outcome.phase === "paused" ? "paused" : "ok");
      }
    },
    [guard, update],
  );
  const runRef = useRef(run);
  runRef.current = run;
  const queue = useRef<RefreshQueue<Slice> | null>(null);
  if (queue.current === null) {
    queue.current = createRefreshQueue<Slice>((slices) => runRef.current(slices), debounceMs);
  }

  const refresh = useCallback(
    (kind: RefreshKind = "all"): Promise<void> =>
      queue.current?.refreshNow(KIND_SLICES[kind]) ?? Promise.resolve(),
    [],
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
      queue.current?.clear();
    };
  }, [pollMs]);

  const applyEvent = useCallback(
    (ev: WsEvent) => {
      const reduced = reduceEvent(snapshotRef.current, ev);
      guard.touch(...reduced.touched);
      update(() => reduced.next);
      if (reduced.inputStopped) setActionError({ message: INPUT_STOPPED_COPY, at: Date.now() });
      if (reduced.serviceEnabled !== undefined) {
        setConnection(reduced.serviceEnabled ? "ok" : "paused");
      }
      if (reduced.refresh.length > 0) queue.current?.schedule(reduced.refresh);
    },
    [guard, update],
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

  const reportError = useCallback((err: unknown) => {
    setActionError({ message: errorCopy(err), at: Date.now() });
  }, []);
  const clearError = useCallback(() => setActionError(null), []);

  const patch = useMemo(
    () => ({
      cd: (next: CdStatus) => {
        guard.touch("cd");
        update((prev) => ({ ...prev, cd: next }));
      },
      activeOutput: (next: ActiveOutputView | null) => {
        guard.touch("activeOutput");
        update((prev) => ({ ...prev, activeOutput: next }));
      },
      activeInput: (next: string | null) => {
        guard.touch("activeInput");
        update((prev) => ({ ...prev, activeInput: next }));
      },
    }),
    [guard, update],
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
