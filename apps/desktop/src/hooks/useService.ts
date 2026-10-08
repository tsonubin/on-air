import { useCallback, useEffect, useRef, useState } from "react";
import { inTauri, invokeCommand, listenToShell } from "../lib/tauri";
import type { CoreSnapshotHandle } from "./useCoreSnapshot";

export interface ServiceHandle {
  /** The audio service is off: capture and output are stopped, `Paired` answers 503. */
  paused: boolean;
  /** Whether this window can turn the service back on (only inside the shell). */
  canResume: boolean;
  resuming: boolean;
  resume(): Promise<void>;
}

/**
 * Service on/off, driven by `/api/status.service_enabled` and the shell's
 * `set_service_enabled` command (the tray menu flips the same flag).
 */
export function useService(
  core: Pick<CoreSnapshotHandle, "snapshot" | "connection" | "refresh" | "reportError">,
): ServiceHandle {
  const { snapshot, connection, refresh, reportError } = core;
  const [resuming, setResuming] = useState(false);
  const paused = connection === "paused" || snapshot.status?.service_enabled === false;

  // The event socket is refused while paused, so the shell's own
  // "service-changed" event (tray toggle or this window) is how we hear about
  // a resume without waiting for the next poll.
  const refreshRef = useRef(refresh);
  refreshRef.current = refresh;
  useEffect(() => {
    let cancelled = false;
    let stop: (() => void) | undefined;
    void listenToShell<boolean>("service-changed", () => {
      void refreshRef.current("all").catch(() => undefined);
    })
      .then((unlisten) => {
        if (cancelled) unlisten();
        else stop = unlisten;
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
      stop?.();
    };
  }, []);

  const resume = useCallback(async () => {
    if (resuming) return;
    setResuming(true);
    try {
      const enabled = await invokeCommand<boolean>("set_service_enabled", { enabled: true });
      if (enabled) await refresh("all");
    } catch (err) {
      reportError(err);
    } finally {
      setResuming(false);
    }
  }, [refresh, reportError, resuming]);

  return { paused, canResume: inTauri(), resuming, resume };
}
