import * as Slider from "@radix-ui/react-slider";
import { useEffect, useRef, useState } from "react";

type Props = {
  label: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  testId: string;
  readout?: string;
  showStepButtons?: boolean;
  onChange: (value: number) => void;
};

export function Fader({
  label,
  value,
  min,
  max,
  step = 1,
  testId,
  readout,
  showStepButtons = false,
  onChange,
}: Props) {
  const [draft, setDraft] = useState(value);
  const draftRef = useRef(value);
  useEffect(() => {
    draftRef.current = value;
    setDraft(value);
  }, [value]);

  const preview = (next: number) => {
    draftRef.current = next;
    setDraft(next);
  };
  const stepBy = (delta: number) => {
    const next = Math.min(max, Math.max(min, draftRef.current + delta));
    preview(next);
    onChange(next);
  };

  return (
    <div className="flex w-10 flex-col items-center gap-1.5 font-mono text-[10px] uppercase tracking-[0.12em] text-steel-dim [@media(max-height:560px)]:gap-1">
      <span className="h-4 text-amber">{readout ?? draft}</span>
      <input
        type="range"
        data-testid={testId}
        min={min}
        max={max}
        step={step}
        value={draft}
        aria-hidden="true"
        tabIndex={-1}
        onChange={(e) => onChange(Number(e.target.value))}
        className="pointer-events-none absolute h-px w-px overflow-hidden opacity-0"
      />
      <Slider.Root
        orientation="vertical"
        min={min}
        max={max}
        step={step}
        value={[draft]}
        onValueChange={([next]) => preview(next ?? min)}
        onValueCommit={([next]) => onChange(next ?? min)}
        className="relative flex h-24 w-6 touch-none select-none flex-col items-center justify-center [@media(max-height:560px)]:h-14"
      >
        <Slider.Track className="relative h-full w-1.5 grow rounded-full bg-[#0c0b0a] shadow-[inset_0_0_0_1px_#2a261f]">
          <Slider.Range className="absolute w-full rounded-full bg-[#5c4a32]" />
        </Slider.Track>
        <Slider.Thumb className="fader-thumb" />
      </Slider.Root>
      {showStepButtons && (
        <div className="flex gap-1">
          <button
            type="button"
            aria-label={`Decrease ${label}`}
            data-testid={`${testId}-down`}
            onClick={() => stepBy(-step)}
            className="flex h-6 w-7 items-center justify-center rounded-sm border border-[#3a342a] bg-[#1a1814] text-sm text-steel hover:border-amber hover:text-amber focus-visible:outline-2 focus-visible:outline-amber"
          >
            ↓
          </button>
          <button
            type="button"
            aria-label={`Increase ${label}`}
            data-testid={`${testId}-up`}
            onClick={() => stepBy(step)}
            className="flex h-6 w-7 items-center justify-center rounded-sm border border-[#3a342a] bg-[#1a1814] text-sm text-steel hover:border-amber hover:text-amber focus-visible:outline-2 focus-visible:outline-amber"
          >
            ↑
          </button>
        </div>
      )}
      <span>{label}</span>
    </div>
  );
}
