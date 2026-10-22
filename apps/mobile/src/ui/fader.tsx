import { type ReactNode, useEffect, useRef, useState } from "react";
import { type LayoutChangeEvent, PanResponder, View, type ViewStyle } from "react-native";
import { useFaderDraft } from "@/hooks/useFaderDraft";
import { colors, layout, radius } from "@/theme";
import { wellStyle } from "./led-window";

/** Where a drag of `delta` points along a track of `length` lands, from `start`. */
export function dragValue({
  start,
  delta,
  length,
  min,
  max,
  step,
}: {
  start: number;
  delta: number;
  length: number;
  min: number;
  max: number;
  step: number;
}): number {
  if (length <= 0) return start;
  const next = start + (delta / length) * (max - min);
  return Math.round(Math.min(max, Math.max(min, next)) / step) * step;
}

export type FaderProps = {
  orientation: "horizontal" | "vertical";
  value: number;
  min: number;
  max: number;
  step: number;
  /** Where the amber fill starts: the bottom of the range, or 0 dB at the center. */
  origin: number;
  /** How far one VoiceOver / TalkBack swipe moves the value. */
  accessibilityStep: number;
  accessibilityLabel: string;
  /** The value as it should be spoken. */
  formatValue: (value: number) => string;
  disabled?: boolean;
  /** Track length in pt (vertical only; a horizontal track measures itself). */
  length?: number;
  /** Drawn above a vertical track, following the drag. */
  renderReadout?: (draft: number) => ReactNode;
  /** Told the shown (clamped) value whenever it moves, so a readout elsewhere can follow the drag. */
  onDraft?: (shown: number) => void;
  testID: string;
  onChange: (value: number) => void | Promise<void>;
};

const SLOT = 6;
const FILL = 3;
const CAP_LONG = 40;
const CAP_SHORT = 20;

const pct = (fraction: number) => `${+(fraction * 100).toFixed(3)}%` as const;

/**
 * A hardware fader drawn on the faceplate: a recessed slot, an amber fill from
 * the origin, and a machined cap. A touch anywhere grabs it, and the value
 * moves with the finger relative to where the touch began, never jumping to
 * the touch point. It commits like the native slider did (`useFaderDraft`).
 */
export function Fader(props: FaderProps) {
  const { orientation, min, max, step, origin, disabled = false, testID } = props;
  const horizontal = orientation === "horizontal";
  const { draft, preview, hold, letGo } = useFaderDraft({
    value: props.value,
    min,
    max,
    step,
    onChange: props.onChange,
  });
  const [width, setWidth] = useState(0);
  const trackLength = horizontal ? width : (props.length ?? 0);

  // The responder lives for the component's lifetime; it reads the latest
  // values through this ref so a re-render mid-drag never loses the gesture.
  const live = useRef({
    draft,
    disabled,
    trackLength,
    preview,
    hold,
    letGo,
    min,
    max,
    step,
    horizontal,
  });
  live.current = { draft, disabled, trackLength, preview, hold, letGo, min, max, step, horizontal };
  const start = useRef(draft);
  const responder = useRef(
    PanResponder.create({
      onStartShouldSetPanResponder: () => !live.current.disabled,
      onMoveShouldSetPanResponder: () => !live.current.disabled,
      onPanResponderTerminationRequest: () => false,
      onPanResponderGrant: () => {
        start.current = live.current.draft;
        live.current.hold();
      },
      onPanResponderRelease: () => live.current.letGo(),
      onPanResponderTerminate: () => live.current.letGo(),
      onPanResponderMove: (_event, gesture) => {
        const now = live.current;
        if (now.disabled) return;
        now.preview(
          dragValue({
            start: start.current,
            delta: now.horizontal ? gesture.dx : -gesture.dy,
            length: now.trackLength,
            min: now.min,
            max: now.max,
            step: now.step,
          }),
        );
      },
    }),
  ).current;

  // A disabled fader gets no release event, so it lets go of any gesture in progress.
  useEffect(() => {
    if (disabled) live.current.letGo();
  }, [disabled]);

  const shown = Math.min(max, Math.max(min, draft));
  const onDraft = useRef(props.onDraft);
  onDraft.current = props.onDraft;
  useEffect(() => {
    onDraft.current?.(shown);
  }, [shown]);
  const fraction = (shown - min) / (max - min);
  const originFraction = (Math.min(max, Math.max(min, origin)) - min) / (max - min);
  const lo = Math.min(fraction, originFraction);
  const hi = Math.max(fraction, originFraction);

  const slot: ViewStyle = horizontal
    ? { position: "absolute", left: 0, right: 0, height: SLOT, top: (CAP_LONG - SLOT) / 2 }
    : { position: "absolute", top: 0, bottom: 0, width: SLOT, left: (CAP_LONG - SLOT) / 2 };
  const fill: ViewStyle = horizontal
    ? {
        position: "absolute",
        left: pct(lo),
        width: pct(hi - lo),
        height: FILL,
        top: (CAP_LONG - FILL) / 2,
      }
    : {
        position: "absolute",
        top: pct(1 - hi),
        height: pct(hi - lo),
        width: FILL,
        left: (CAP_LONG - FILL) / 2,
      };
  const cap: ViewStyle = horizontal
    ? {
        position: "absolute",
        left: pct(fraction),
        marginLeft: -CAP_SHORT / 2,
        top: 0,
        width: CAP_SHORT,
        height: CAP_LONG,
      }
    : {
        position: "absolute",
        top: pct(1 - fraction),
        marginTop: -CAP_SHORT / 2,
        left: 0,
        width: CAP_LONG,
        height: CAP_SHORT,
      };

  return (
    <View
      testID={testID}
      accessible
      accessibilityRole="adjustable"
      accessibilityLabel={props.accessibilityLabel}
      accessibilityState={disabled ? { disabled: true } : undefined}
      accessibilityActions={[{ name: "increment" }, { name: "decrement" }]}
      accessibilityValue={{ min, max, now: shown, text: props.formatValue(shown) }}
      onAccessibilityAction={
        disabled
          ? undefined
          : (event) => {
              const direction = event.nativeEvent.actionName === "increment" ? 1 : -1;
              preview(draft + direction * props.accessibilityStep);
            }
      }
      {...(disabled ? {} : responder.panHandlers)}
      style={
        horizontal
          ? { minHeight: layout.minTarget, justifyContent: "center" }
          : { minWidth: layout.minTarget, alignItems: "center", gap: 10 }
      }
    >
      {!horizontal && props.renderReadout ? props.renderReadout(shown) : null}
      <View
        testID={`${testID}-track`}
        onLayout={
          horizontal ? (e: LayoutChangeEvent) => setWidth(e.nativeEvent.layout.width) : undefined
        }
        style={
          horizontal
            ? { height: CAP_LONG, alignSelf: "stretch" }
            : { width: CAP_LONG, height: trackLength }
        }
      >
        <View style={[wellStyle, { borderRadius: SLOT / 2 }, slot]} />
        {disabled ? null : (
          <View
            testID={`${testID}-fill`}
            style={[{ backgroundColor: colors.amber, borderRadius: FILL / 2 }, fill]}
          />
        )}
        <View
          testID={`${testID}-cap`}
          style={[
            {
              borderRadius: radius.sm,
              experimental_backgroundImage: `linear-gradient(180deg, ${colors.capTop}, ${colors.capBottom})`,
              boxShadow: "0 2px 3px rgba(0,0,0,0.6)",
              // The index line stretches along the cross axis, so the cap lays out
              // across the travel: a row for a horizontal fader, a column for a vertical one.
              flexDirection: horizontal ? "row" : "column",
              alignItems: "center",
              justifyContent: "center",
            },
            cap,
            disabled && { opacity: 0.45 },
          ]}
        >
          <View
            testID={`${testID}-cap-line`}
            style={
              horizontal
                ? {
                    width: 1,
                    alignSelf: "stretch",
                    marginVertical: 6,
                    backgroundColor: colors.label,
                  }
                : {
                    height: 1,
                    alignSelf: "stretch",
                    marginHorizontal: 6,
                    backgroundColor: colors.label,
                  }
            }
          />
        </View>
      </View>
    </View>
  );
}
