import * as Select from "@radix-ui/react-select";
import { useId } from "react";

type Props = {
  label: string;
  /** Accessible name; defaults to `label`. */
  ariaLabel?: string;
  value: number;
  options: number[];
  testId: string;
  disabled?: boolean;
  onChange: (hz: number) => void;
};

export function RateSelect({
  label,
  ariaLabel,
  value,
  options,
  testId,
  disabled = false,
  onChange,
}: Props) {
  const labelId = useId();
  return (
    <div className="flex items-center justify-between gap-3 font-mono text-xs uppercase tracking-[0.14em] text-steel-dim">
      <span id={labelId}>
        {ariaLabel ? (
          <>
            <span aria-hidden="true">{label}</span>
            <span className="sr-only">{ariaLabel}</span>
          </>
        ) : (
          label
        )}
      </span>
      {/* Test hook: Playwright drives a native <select>; users get the Radix one. */}
      <select
        data-testid={testId}
        value={value}
        disabled={disabled}
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
      <Select.Root
        value={String(value)}
        disabled={disabled}
        onValueChange={(v) => onChange(Number(v))}
      >
        <Select.Trigger
          aria-labelledby={labelId}
          className="inline-flex min-w-[108px] items-center justify-between gap-2 rounded border border-line bg-well px-2 py-1.5 font-mono text-sm normal-case tracking-normal text-ink outline-none hover:border-amber focus-visible:border-amber data-disabled:cursor-not-allowed data-disabled:opacity-60 data-[state=open]:border-amber"
        >
          <Select.Value />
          <Select.Icon aria-hidden="true">▾</Select.Icon>
        </Select.Trigger>
        <Select.Portal>
          <Select.Content
            position="popper"
            sideOffset={4}
            className="z-80 min-w-(--radix-select-trigger-width) overflow-hidden rounded border border-line bg-sign-bottom text-ink shadow-[0_12px_32px_var(--color-shadow-deep)]"
          >
            <Select.Viewport>
              {options.map((hz) => (
                <Select.Item
                  key={hz}
                  value={String(hz)}
                  className="cursor-pointer rounded px-2 py-1.5 font-mono text-sm text-ink outline-none data-highlighted:bg-face-hover data-[state=checked]:text-amber"
                >
                  <Select.ItemText>{hz}</Select.ItemText>
                </Select.Item>
              ))}
            </Select.Viewport>
          </Select.Content>
        </Select.Portal>
      </Select.Root>
    </div>
  );
}
