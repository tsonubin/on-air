import type { DeviceHelpTopic } from "../hooks/useOutputSelection";
import { Button } from "../ui/Button";

const COPY: Record<DeviceHelpTopic, { title: string; body: string }> = {
  bluetooth: {
    title: "Connect a Bluetooth speaker",
    body: "Pair and connect your speaker in the desktop’s Bluetooth settings. Return here, refresh devices, then select the speaker. Opening settings does not start playback.",
  },
  airplay: {
    title: "AirPlay on this Mac",
    body: "AirPlay playback from on-air is not available on this Mac yet. The macOS picker can open, but choosing a speaker there does not start this mixer. Use Bluetooth or Sonos for playback.",
  },
};

export function DeviceHelp({
  topic,
  refreshing,
  onRefresh,
  onOpenPicker,
  onDismiss,
}: {
  topic: DeviceHelpTopic | null;
  refreshing: boolean;
  onRefresh(): void;
  onOpenPicker(): void;
  onDismiss(): void;
}) {
  if (!topic) return null;
  const copy = COPY[topic];
  return (
    <aside className="device-help" aria-label="Speaker setup" role="status">
      <div>
        <strong>{copy.title}</strong>
        <p>{copy.body}</p>
      </div>
      <div className="device-actions">
        {topic === "bluetooth" ? (
          <Button onClick={onRefresh} disabled={refreshing} aria-busy={refreshing}>
            {refreshing ? "Refreshing…" : "Refresh devices"}
          </Button>
        ) : (
          <Button data-testid="airplay-picker" onClick={onOpenPicker}>
            Open macOS picker
          </Button>
        )}
        <Button onClick={onDismiss}>Dismiss</Button>
      </div>
    </aside>
  );
}
