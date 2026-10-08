import { useCallback, useEffect, useState } from "react";
import { inTauri, invokeCommand, listenToShell } from "../lib/tauri";

export interface AutostartHandle {
  /** `null` outside the Tauri shell (plain browser, Playwright) or before the first read. */
  autostart: boolean | null;
  toggle(): Promise<void>;
}

/** Login-item preference, via the shell's `autostart_enabled` / `set_autostart` commands. */
export function useAutostart(reportError: (err: unknown) => void): AutostartHandle {
  const [autostart, setAutostart] = useState<boolean | null>(null);

  useEffect(() => {
    if (!inTauri()) return;
    let cancelled = false;
    let stop: (() => void) | undefined;
    void invokeCommand<boolean>("autostart_enabled")
      .then((enabled) => {
        if (!cancelled) setAutostart(enabled);
      })
      .catch(() => {
        if (!cancelled) setAutostart(null);
      });
    void listenToShell<boolean>("autostart-changed", (enabled) => setAutostart(enabled))
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

  const toggle = useCallback(async () => {
    if (autostart === null) return;
    try {
      const next = await invokeCommand<boolean>("set_autostart", { enabled: !autostart });
      setAutostart(next);
    } catch (err) {
      reportError(err);
    }
  }, [autostart, reportError]);

  return { autostart, toggle };
}
