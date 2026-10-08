import type { ConnectionPhase } from "@/hooks/useDesktopConnection";

/**
 * The one state every label and enabled flag is derived from.
 * `ready`: connected, nothing casting. `live`: connected and casting.
 */
export type UiState =
  | "restoring"
  | "pairing"
  | "connecting"
  | "reconnecting"
  | "paused"
  | "ready"
  | "live";

export function deriveUiState({
  hydrated,
  paired,
  phase,
  casting,
}: {
  hydrated: boolean;
  paired: boolean;
  phase: ConnectionPhase;
  casting: boolean;
}): UiState {
  if (!hydrated) return "restoring";
  if (!paired) return "pairing";
  switch (phase) {
    case "idle":
    case "connecting":
      return "connecting";
    case "reconnecting":
      return "reconnecting";
    case "paused":
      return "paused";
    case "unauthorized":
      return "pairing";
    case "connected":
      return casting ? "live" : "ready";
  }
}

export type UiView = {
  /** Short pill text in the mixer header. */
  statusText: string;
  /** What the status pill announces. */
  statusAnnouncement: string;
  /** Sentence on the Connection screen. */
  connectionText: string;
  live: boolean;
  /** Source, speaker, EQ and sample-rate controls accept input. */
  controlsEnabled: boolean;
  /** The volume fader accepts input (something must be casting). */
  volumeEnabled: boolean;
  /** Sound settings can be opened (they explain a reconnect, not a pause). */
  soundEnabled: boolean;
  paused: boolean;
};

const statusText: Record<UiState, string> = {
  restoring: "Restoring",
  pairing: "Not paired",
  connecting: "Connecting",
  reconnecting: "Reconnecting",
  paused: "Paused",
  ready: "Ready",
  live: "Live",
};

const connectionText: Record<UiState, string> = {
  restoring: "Restoring…",
  pairing: "Not paired",
  connecting: "Connecting…",
  reconnecting: "Reconnecting…",
  paused: "Connected to desktop",
  ready: "Connected to desktop",
  live: "Connected to desktop",
};

export function describeUiState(state: UiState): UiView {
  const connected = state === "ready" || state === "live";
  return {
    statusText: statusText[state],
    statusAnnouncement:
      state === "live"
        ? "Audio is live"
        : state === "paused"
          ? "Desktop service is paused"
          : `Status: ${statusText[state]}`,
    connectionText: connectionText[state],
    live: state === "live",
    controlsEnabled: connected,
    volumeEnabled: state === "live",
    soundEnabled: state !== "paused",
    paused: state === "paused",
  };
}
