import * as Slider from "@radix-ui/react-slider";

type Props = {
  label: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  testId: string;
  readout?: string;
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
  onChange,
}: Props) {
  return (
    <div className="flex w-10 flex-col items-center gap-1.5 font-mono text-[10px] uppercase tracking-[0.12em] text-steel-dim [@media(max-height:560px)]:gap-1">
      <span className="h-4 text-amber">{readout ?? value}</span>
      <input
        type="range"
        data-testid={testId}
        min={min}
        max={max}
        step={step}
        value={value}
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
        value={[value]}
        onValueChange={([next]) => onChange(next ?? min)}
        className="relative flex h-24 w-[22px] touch-none select-none flex-col items-center justify-center [@media(max-height:560px)]:h-14"
      >
        <Slider.Track className="relative h-full w-1.5 grow rounded-full bg-[#0c0b0a] shadow-[inset_0_0_0_1px_#2a261f]">
          <Slider.Range className="absolute w-full rounded-full bg-[#5c4a32]" />
        </Slider.Track>
        <Slider.Thumb className="fader-thumb block h-3.5 w-[18px] cursor-grab rounded-sm" />
      </Slider.Root>
      <span>{label}</span>
    </div>
  );
}
