import type { SyncState } from "@on-air/api-types";

/** How each sync state is spelled in the clients. */
export const SYNC_STATE_LABELS: Record<SyncState, string> = {
  free_running: "Free-running",
  syncing: "Syncing",
  locked: "Locked",
};

/** The core accepts a trim within ±10 s (`sync::latency::TRIM_LIMIT_MS`). */
export const SYNC_TRIM_LIMIT_MS = 10_000;

export const SYNC_TRIM_DESCRIPTION =
  "Manual correction in milliseconds against what you actually hear. It is not measured or calibrated. It applies the next time this output plays.";

export const SYNC_DRIFT_DESCRIPTION =
  "Drift is measured against the shared timeline while an output is synchronized.";
