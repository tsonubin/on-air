import type { MixerHandle } from "../hooks/useMixer";
import { Fader } from "../ui/Fader";
import { RateSelect } from "../ui/RateSelect";

const EQ_BANDS = [
  { label: "60", name: "60 hertz" },
  { label: "250", name: "250 hertz" },
  { label: "1k", name: "1 kilohertz" },
  { label: "4k", name: "4 kilohertz" },
  { label: "12k", name: "12 kilohertz" },
] as const;

export function MixerFooter({ mixer, disabled }: { mixer: MixerHandle; disabled: boolean }) {
  return (
    <footer className="grid shrink-0 grid-cols-1 items-end justify-items-center gap-4 border-t border-line-soft bg-footer px-4 py-3 min-[561px]:grid-cols-[auto_minmax(0,1fr)_minmax(9.5rem,11rem)] min-[561px]:justify-items-stretch [@media(max-height:560px)]:gap-2.5 [@media(max-height:560px)]:px-3 [@media(max-height:560px)]:py-2">
      <Fader
        label="volume"
        ariaLabel="Volume"
        value={mixer.volume}
        min={0}
        max={100}
        testId="volume-slider"
        showStepButtons
        disabled={disabled}
        onChange={mixer.setVolume}
      />
      <fieldset className="m-0 flex min-w-0 items-end justify-center gap-3 border-0 p-0">
        <legend className="sr-only">Equalizer</legend>
        {mixer.gains.map((gain, i) => {
          const band = EQ_BANDS[i];
          return (
            <Fader
              key={band.label}
              label={band.label}
              ariaLabel={`Equalizer ${band.name}`}
              value={gain}
              min={-12}
              max={12}
              step={0.5}
              testId={`eq-band-${i}`}
              disabled={disabled}
              onChange={(v) => mixer.setGain(i, v)}
            />
          );
        })}
      </fieldset>
      <div className="flex w-full min-w-[11rem] flex-col justify-end gap-2">
        <RateSelect
          label="In"
          ariaLabel="Input sample rate"
          value={mixer.inputHz}
          options={mixer.inputRates}
          testId="sample-rate"
          disabled={disabled}
          onChange={mixer.setInputRate}
        />
        <RateSelect
          label="Out"
          ariaLabel="Output sample rate"
          value={mixer.outputHz}
          options={mixer.outputRates}
          testId="output-sample-rate"
          disabled={disabled}
          onChange={mixer.setOutputRate}
        />
      </div>
    </footer>
  );
}
