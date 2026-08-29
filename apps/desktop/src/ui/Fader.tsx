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
    <div className="fader">
      <span className="fader-readout">{readout ?? value}</span>
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
        className="visually-hidden-control"
      />
      <Slider.Root
        orientation="vertical"
        min={min}
        max={max}
        step={step}
        value={[value]}
        onValueChange={([next]) => onChange(next ?? min)}
        className="fader-rail"
      >
        <Slider.Track className="fader-track">
          <Slider.Range className="fader-range" />
        </Slider.Track>
        <Slider.Thumb className="fader-thumb" />
      </Slider.Root>
      <span>{label}</span>
    </div>
  );
}
