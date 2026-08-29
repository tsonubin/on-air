import * as Select from "@radix-ui/react-select";

type Props = {
  label: string;
  value: number;
  options: number[];
  testId: string;
  onChange: (hz: number) => void;
};

export function RateSelect({ label, value, options, testId, onChange }: Props) {
  return (
    <label className="flex items-center justify-between gap-3 font-mono text-[10px] uppercase tracking-[0.14em] text-steel-dim">
      {label}
      <select
        data-testid={testId}
        value={value}
        aria-hidden="true"
        tabIndex={-1}
        onChange={(e) => onChange(Number(e.target.value))}
        className="pointer-events-none absolute h-px w-px overflow-hidden opacity-0"
      >
        {options.map((hz) => (
          <option key={hz} value={hz}>
            {hz}
          </option>
        ))}
      </select>
      <Select.Root value={String(value)} onValueChange={(v) => onChange(Number(v))}>
        <Select.Trigger className="inline-flex min-w-[108px] items-center justify-between gap-2 rounded border border-[#3a342a] bg-well px-2 py-1.5 font-mono text-[11px] normal-case tracking-normal text-ink outline-none hover:border-amber data-[state=open]:border-amber">
          <Select.Value />
          <Select.Icon>▾</Select.Icon>
        </Select.Trigger>
        <Select.Portal>
          <Select.Content
            position="popper"
            sideOffset={4}
            className="z-80 min-w-(--radix-select-trigger-width) overflow-hidden rounded border border-[#3a342a] bg-[#16140f] text-ink shadow-[0_12px_32px_rgba(0,0,0,0.55)]"
          >
            <Select.Viewport>
              {options.map((hz) => (
                <Select.Item
                  key={hz}
                  value={String(hz)}
                  className="cursor-pointer rounded px-2 py-1.5 font-mono text-[11px] text-ink outline-none data-[highlighted]:bg-[#2a2218] data-[state=checked]:text-amber"
                >
                  <Select.ItemText>{hz}</Select.ItemText>
                </Select.Item>
              ))}
            </Select.Viewport>
          </Select.Content>
        </Select.Portal>
      </Select.Root>
    </label>
  );
}
