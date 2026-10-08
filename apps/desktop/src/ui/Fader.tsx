import * as Slider from "@radix-ui/react-slider";
import { useEffect, useRef, useState } from "react";
import { Button } from "./Button";

type Props = {
  label: string;
  /** Accessible name for the thumb; defaults to `label`. */
  ariaLabel?: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  testId: string;
  readout?: string;
  showStepButtons?: boolean;
  disabled?: boolean;
  onChange: (value: number) => void;
};

export function Fader({
  label,
  ariaLabel = label,
  value,
  min,
  max,
  step = 1,
  testId,
  readout,
  showStepButtons = false,
  disabled = false,
  onChange,
}: Props) {
  const [draft, setDraft] = useState(value);
  const draftRef = useRef(value);
  // While the thumb is held, polls and echoes must not yank it back.
  const dragging = useRef(false);
  useEffect(() => {
    if (dragging.current) return;
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
    <div className="flex w-10 flex-col items-center gap-1.5 font-mono text-xs uppercase tracking-[0.12em] text-steel-dim [@media(max-height:560px)]:gap-1">
      <span className="h-4 text-amber" aria-hidden="true">
        {readout ?? draft}
      </span>
      <input
        type="range"
        data-testid={testId}
        min={min}
        max={max}
        step={step}
        value={draft}
        disabled={disabled}
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
        disabled={disabled}
        onValueChange={([next]) => {
          dragging.current = true;
          preview(next ?? min);
        }}
        onValueCommit={([next]) => {
          dragging.current = false;
          onChange(next ?? min);
        }}
        className="relative flex h-24 w-6 touch-none select-none flex-col items-center justify-center data-disabled:opacity-60 [@media(max-height:560px)]:h-14"
      >
        <Slider.Track className="relative h-full w-1.5 grow rounded-full bg-well shadow-[inset_0_0_0_1px_var(--color-face-raised)]">
          <Slider.Range className="absolute w-full rounded-full bg-key" />
        </Slider.Track>
        <Slider.Thumb className="fader-thumb" aria-label={ariaLabel} />
      </Slider.Root>
      {showStepButtons && (
        <div className="flex gap-1">
          <Button
            variant="step"
            aria-label={`Decrease ${ariaLabel}`}
            data-testid={`${testId}-down`}
            disabled={disabled}
            onClick={() => stepBy(-step)}
          >
            ↓
          </Button>
          <Button
            variant="step"
            aria-label={`Increase ${ariaLabel}`}
            data-testid={`${testId}-up`}
            disabled={disabled}
            onClick={() => stepBy(step)}
          >
            ↑
          </Button>
        </div>
      )}
      <span aria-hidden="true">{label}</span>
    </div>
  );
}
