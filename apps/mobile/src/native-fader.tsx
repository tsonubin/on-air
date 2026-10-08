import { Column, Row, Slider, Spacer, Text } from "@expo/ui";
import { accessibilityLabel as a11yLabel } from "@expo/ui/swift-ui/modifiers";
import { useEffect, useRef, useState } from "react";
import { Platform } from "react-native";
import { theme } from "./theme";

/**
 * A native slider that previews locally and commits after a short pause. While
 * the person is dragging, and for a moment after the last commit settles,
 * incoming `value` props (from polling or events) are ignored so the thumb
 * never snaps back mid-gesture; the latest prop is applied once they let go.
 */
export function NativeFader(props: {
  label: string;
  /** Spoken name of the slider; defaults to `label`. */
  accessibilityLabel?: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  testId: string;
  disabled?: boolean;
  hideHeader?: boolean;
  unit?: string;
  signed?: boolean;
  onChange: (value: number) => void | Promise<void>;
}) {
  const quantum = props.step ?? 1;
  const clamp = (value: number) => {
    const bounded = Math.min(props.max, Math.max(props.min, value));
    return Math.round(bounded / quantum) * quantum;
  };
  const [draft, setDraft] = useState(props.value);
  const interacting = useRef(false);
  const latestProp = useRef(props.value);
  latestProp.current = props.value;
  const commitTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const settleTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const onChange = useRef(props.onChange);
  onChange.current = props.onChange;

  useEffect(() => {
    if (!interacting.current) setDraft(props.value);
  }, [props.value]);

  useEffect(
    () => () => {
      if (commitTimer.current) clearTimeout(commitTimer.current);
      if (settleTimer.current) clearTimeout(settleTimer.current);
    },
    [],
  );

  const release = () => {
    if (settleTimer.current) clearTimeout(settleTimer.current);
    settleTimer.current = setTimeout(() => {
      settleTimer.current = null;
      interacting.current = false;
      setDraft(latestProp.current);
    }, theme.motion.interactionSettleMs);
  };

  const commit = async (value: number) => {
    commitTimer.current = null;
    try {
      await onChange.current(value);
    } catch {
      // The owner reports write failures; the fader only needs to let go.
    }
    // A newer preview restarted the cycle; let that one release.
    if (commitTimer.current === null) release();
  };

  const preview = (nextValue: number) => {
    const next = clamp(nextValue);
    interacting.current = true;
    if (settleTimer.current) clearTimeout(settleTimer.current);
    settleTimer.current = null;
    setDraft(next);
    if (commitTimer.current) clearTimeout(commitTimer.current);
    commitTimer.current = setTimeout(() => void commit(next), theme.motion.controlCommitMs);
  };

  const spoken = props.accessibilityLabel ?? props.label;
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
        modifiers={Platform.OS === "ios" ? [a11yLabel(spoken)] : undefined}
        testID={`${props.testId}-native`}
      />
    </Column>
  );
}
