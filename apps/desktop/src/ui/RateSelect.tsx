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
    <label className="rate-row">
      {label}
      <select
        data-testid={testId}
        value={value}
        aria-hidden="true"
        tabIndex={-1}
        onChange={(e) => onChange(Number(e.target.value))}
        className="visually-hidden-control"
      >
        {options.map((hz) => (
          <option key={hz} value={hz}>
            {hz}
          </option>
        ))}
      </select>
      <Select.Root value={String(value)} onValueChange={(v) => onChange(Number(v))}>
        <Select.Trigger className="rate-trigger">
          <Select.Value />
          <Select.Icon>▾</Select.Icon>
        </Select.Trigger>
        <Select.Portal>
          <Select.Content
            position="popper"
            sideOffset={4}
            className="rate-menu"
          >
            <Select.Viewport>
              {options.map((hz) => (
                <Select.Item
                  key={hz}
                  value={String(hz)}
                  className="rate-item"
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
