import type { CdStatus } from "@on-air/api-types";
import { Button } from "./Button";

function padTrack(n: number): string {
  return String(n).padStart(2, "0");
}

export function CdTransport({
  status,
  disabled = false,
  onPlayPause,
  onPrev,
  onNext,
}: {
  status: CdStatus | null;
  disabled?: boolean;
  onPlayPause: () => void;
  onPrev: () => void;
  onNext: () => void;
}) {
  if (!status?.present) return null;
  const label = status.title || status.album || "Audio CD";
  return (
    <section
      className="panel-raised mx-3 mt-2 flex min-h-[46px] shrink-0 flex-wrap items-center gap-2.5 rounded-md border border-line px-2.5 py-1.5"
      data-testid="cd-transport"
      aria-label="Compact disc"
    >
      <span className="font-mono text-xs tracking-[0.18em] text-steel-dim uppercase">cd</span>
      <span
        className="led-readout relative inline-grid rounded-sm px-1.5 py-0.5 text-base tracking-[0.08em]"
        data-testid="cd-track"
      >
        <span className="col-start-1 row-start-1 text-led-ghost select-none" aria-hidden="true">
          88/88
        </span>
        <span className="led-glow col-start-1 row-start-1 text-amber">
          {padTrack(status.track)}/{padTrack(status.track_count)}
        </span>
      </span>
      <span className="min-w-0 flex-1 truncate text-base text-ink" title={label}>
        {label}
      </span>
      <div className="flex items-center gap-1">
        <Button
          variant="key"
          data-testid="cd-prev"
          aria-label="Previous track"
          disabled={disabled}
          onClick={onPrev}
        >
          ⏮
        </Button>
        <Button
          variant="key-main"
          data-testid="cd-play"
          aria-label={status.playing ? "Pause" : "Play"}
          disabled={disabled}
          onClick={onPlayPause}
        >
          {status.playing ? "❚❚" : "▶"}
        </Button>
        <Button
          variant="key"
          data-testid="cd-next"
          aria-label="Next track"
          disabled={disabled}
          onClick={onNext}
        >
          ⏭
        </Button>
      </div>
    </section>
  );
}
