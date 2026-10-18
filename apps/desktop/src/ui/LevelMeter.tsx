import type { MeterLevel } from "@on-air/api-types";
import { levelFraction, type StereoLevels } from "@on-air/control-client";
import { useEffect, useState } from "react";

/** Silence for this long reads zero, like the TUI's meter timeout. */
const QUIET_MS = 1_000;

const SILENT: StereoLevels = {
  left: { rms: 0, peak: 0 },
  right: { rms: 0, peak: 0 },
};

function ChannelBar({
  channel,
  level,
  disabled,
}: {
  channel: "L" | "R";
  level: MeterLevel;
  disabled: boolean;
}) {
  const rms = Math.round(levelFraction(level.rms) * 100);
  const peak = Math.round(levelFraction(level.peak) * 100);
  return (
    <div className="flex items-center gap-1.5">
      <span aria-hidden="true" className="w-2 font-mono text-xs text-steel-dim">
        {channel}
      </span>
      <div
        aria-hidden="true"
        className={`relative h-2 flex-1 overflow-hidden rounded-[2px] border border-line-soft bg-well ${
          disabled ? "opacity-50" : ""
        }`}
      >
        <div className="absolute inset-y-0 left-0 bg-live" style={{ width: `${rms}%` }} />
        <div className="absolute inset-y-0 w-px bg-amber" style={{ left: `${peak}%` }} />
      </div>
      <span className="sr-only">
        {`${channel === "L" ? "Left" : "Right"} channel level ${rms} percent, peak ${peak} percent`}
      </span>
    </div>
  );
}

/**
 * The stereo level meter: one RMS bar with a peak tick per channel, fed by
 * `LevelMeter` frames. When frames stop arriving the bars read zero after
 * a second instead of freezing at their last values.
 */
export function LevelMeter({
  levels,
  disabled,
}: {
  levels: StereoLevels | null;
  disabled: boolean;
}) {
  const [fresh, setFresh] = useState(false);
  useEffect(() => {
    if (!levels) return;
    setFresh(true);
    const id = setTimeout(() => setFresh(false), QUIET_MS);
    return () => clearTimeout(id);
  }, [levels]);
  const shown = fresh && levels ? levels : SILENT;
  return (
    <div
      className="flex w-full min-w-[7rem] flex-col justify-end gap-1.5"
      data-testid="level-meter"
    >
      <span className="font-mono text-xs uppercase tracking-[0.12em] text-steel-dim">Level</span>
      <ChannelBar channel="L" level={shown.left} disabled={disabled} />
      <ChannelBar channel="R" level={shown.right} disabled={disabled} />
    </div>
  );
}
