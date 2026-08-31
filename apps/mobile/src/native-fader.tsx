import { Button, Column, Row, Slider, Spacer, Text } from "@expo/ui";
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
  showStepButtons?: boolean;
  onChange: (value: number) => void;
}) {
  const step = props.step ?? 1;
  const clamp = (value: number) => Math.min(props.max, Math.max(props.min, value));
  const [draft, setDraft] = useState(props.value);
  const draftRef = useRef(props.value);
  const commitTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const onChange = useRef(props.onChange);
  onChange.current = props.onChange;

  useEffect(() => {
    draftRef.current = props.value;
    setDraft(props.value);
  }, [props.value]);
  useEffect(
    () => () => {
      if (commitTimer.current) clearTimeout(commitTimer.current);
    },
    [],
  );

  const updateDraft = (nextValue: number) => {
    const next = clamp(nextValue);
    draftRef.current = next;
    setDraft(next);
    return next;
  };
  const changeNow = (nextValue: number) => {
    if (commitTimer.current) clearTimeout(commitTimer.current);
    onChange.current(updateDraft(nextValue));
  };
  const preview = (nextValue: number) => {
    const next = updateDraft(nextValue);
    if (commitTimer.current) clearTimeout(commitTimer.current);
    commitTimer.current = setTimeout(() => onChange.current(next), theme.motion.controlCommitMs);
  };

  return (
    <Column spacing={theme.spacing.sm} testID={props.testId}>
      <Row alignment="center" spacing={theme.spacing.sm}>
        <Text textStyle={{ fontWeight: "600" }}>{props.label}</Text>
        <Spacer />
        <Text textStyle={{ fontWeight: "600" }}>{String(draft)}</Text>
      </Row>
      <Slider
        min={props.min}
        max={props.max}
        step={step}
        value={draft}
        disabled={props.disabled}
        onValueChange={preview}
        testID={`${props.testId}-native`}
      />
      {props.showStepButtons && (
        <Row alignment="center" spacing={theme.spacing.sm}>
          <Button
            label="Lower ↓"
            variant="outlined"
            disabled={props.disabled || draft <= props.min}
            onPress={() => changeNow(draftRef.current - step)}
            testID={`${props.testId}-down`}
          />
          <Spacer />
          <Button
            label="Raise ↑"
            variant="outlined"
            disabled={props.disabled || draft >= props.max}
            onPress={() => changeNow(draftRef.current + step)}
            testID={`${props.testId}-up`}
          />
        </Row>
      )}
    </Column>
  );
}
