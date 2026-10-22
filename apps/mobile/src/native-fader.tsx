import { Column, Row, Slider, Spacer, Text } from "@expo/ui";
import { accessibilityLabel as a11yLabel } from "@expo/ui/swift-ui/modifiers";
import { Platform } from "react-native";
import { useFaderDraft } from "./hooks/useFaderDraft";
import { colors, fonts, theme } from "./theme";

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
  const { draft, preview } = useFaderDraft({
    value: props.value,
    min: props.min,
    max: props.max,
    step: quantum,
    onChange: props.onChange,
  });

  const spoken = props.accessibilityLabel ?? props.label;
  return (
    <Column spacing={theme.spacing.sm} testID={props.testId}>
      {!props.hideHeader && (
        <Row alignment="center" spacing={theme.spacing.sm}>
          <Text textStyle={{ fontFamily: fonts.sansMedium, color: colors.label }}>
            {props.label}
          </Text>
          <Spacer />
          <Text textStyle={{ fontFamily: fonts.monoMedium, color: colors.amber }}>
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
