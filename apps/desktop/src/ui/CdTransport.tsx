import type { CdStatus } from "@on-air/api-types";

function padTrack(n: number): string {
  return String(n).padStart(2, "0");
}

export function CdTransport({
  status,
  onPlayPause,
  onPrev,
  onNext,
}: {
  status: CdStatus;
  onPlayPause: () => void;
  onPrev: () => void;
  onNext: () => void;
}) {
  if (!status.present) return null;
  const label = status.title || status.album || "Audio CD";
  return (
    <section
      className="cd-deck mx-3 mt-2 flex min-h-[46px] shrink-0 flex-wrap items-center gap-2.5 rounded-md border border-[#3a342a] bg-linear-to-b from-[#2a261f] to-[#16140f] px-2.5 py-1.5 shadow-[inset_0_1px_0_#4a4338]"
      data-testid="cd-transport"
      aria-label="Compact disc"
    >
      <span className="font-mono text-[8px] tracking-[0.18em] text-steel-dim uppercase">cd</span>
      <span
        className="relative inline-grid rounded-sm bg-[#070605] px-1.5 py-0.5 font-led text-[13px] leading-none tracking-[0.08em] shadow-[inset_0_1px_3px_#000]"
        data-testid="cd-track"
      >
        <span className="col-start-1 row-start-1 text-[#1c1812] select-none" aria-hidden="true">
          88/88
        </span>
        <span className="col-start-1 row-start-1 text-amber [text-shadow:0_0_4px_rgba(224,162,75,0.8)]">
          {padTrack(status.track)}/{padTrack(status.track_count)}
        </span>
      </span>
      <span className="min-w-0 flex-1 truncate text-[13px] text-ink" title={label}>
        {label}
      </span>
      <div className="flex items-center gap-1">
        <button
          type="button"
          className="cd-key"
          data-testid="cd-prev"
          aria-label="Previous track"
          onClick={onPrev}
        >
          ⏮
        </button>
        <button
          type="button"
          className="cd-key cd-key-main"
          data-testid="cd-play"
          aria-label={status.playing ? "Pause" : "Play"}
          onClick={onPlayPause}
        >
          {status.playing ? "❚❚" : "▶"}
        </button>
        <button
          type="button"
          className="cd-key"
          data-testid="cd-next"
          aria-label="Next track"
          onClick={onNext}
        >
          ⏭
        </button>
      </div>
    </section>
  );
}
