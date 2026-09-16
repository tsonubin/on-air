import { Column, Row, Slider, Spacer, Text } from "@expo/ui";
import { useEffect, useRef, useState } from "react";
import { theme } from "./theme";

export function NativeFader(props: {
  label: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  testId: string;
  disabled?: boolean;
  hideHeader?: boolean;
  unit?: string;
  signed?: boolean;
  onChange: (value: number) => void;
}) {
  const quantum = props.step ?? 1;
  const clamp = (value: number) => {
    const bounded = Math.min(props.max, Math.max(props.min, value));
    return Math.round(bounded / quantum) * quantum;
  };
  const [draft, setDraft] = useState(props.value);
  const commitTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const onChange = useRef(props.onChange);
  onChange.current = props.onChange;

  useEffect(() => {
    setDraft(props.value);
  }, [props.value]);
  useEffect(
    () => () => {
      if (commitTimer.current) clearTimeout(commitTimer.current);
    },
    [],
  );

  const preview = (nextValue: number) => {
    const next = clamp(nextValue);
    setDraft(next);
    if (commitTimer.current) clearTimeout(commitTimer.current);
    commitTimer.current = setTimeout(() => onChange.current(next), theme.motion.controlCommitMs);
  };

  return (
    <Column spacing={theme.spacing.sm} testID={props.testId}>
      {!props.hideHeader && (
        <Row alignment="center" spacing={theme.spacing.sm}>
          <Text textStyle={{ fontWeight: "600" }}>{props.label}</Text>
          <Spacer />
          <Text textStyle={{ fontWeight: "600" }}>
            {`${props.signed && draft > 0 ? "+" : ""}${Number.isInteger(draft) ? String(draft) : draft.toFixed(1)}${props.unit ? ` ${props.unit}` : ""}`}
          </Text>
        </Row>
      )}
      <Slider
        min={props.min}
        max={props.max}
        value={draft}
        step={quantum}
        disabled={props.disabled}
        onValueChange={preview}
        testID={`${props.testId}-native`}
      />
    </Column>
  );
}
