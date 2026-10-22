import { Text, View } from "react-native";
import { colors, fonts, type } from "@/theme";
import { wellStyle } from "./led-window";

/** DSEG14 draws printable ASCII only; anything else (accents, CJK, emoji) needs another face. */
export function isDsegText(text: string): boolean {
  return /^[\x20-\x7E]*$/.test(text);
}

/**
 * One line of 14-segment text, uppercased by style so the string itself stays
 * as given. Names DSEG14 cannot draw fall back to Plex Mono. Long text shrinks
 * a little, then truncates; it never wraps out of its window.
 */
export function LedText({
  children,
  size,
  dim = false,
  testID,
}: {
  children: string;
  size: number;
  dim?: boolean;
  testID?: string;
}) {
  return (
    <Text
      testID={testID}
      numberOfLines={1}
      ellipsizeMode="tail"
      adjustsFontSizeToFit
      minimumFontScale={0.75}
      maxFontSizeMultiplier={MAX_SCALE}
      style={{
        fontFamily: isDsegText(children) ? fonts.led : fonts.monoMedium,
        fontSize: size,
        lineHeight: size * 1.3,
        textTransform: "uppercase",
        color: dim ? colors.tertiaryLabel : colors.amber,
        textShadowColor: colors.amber,
        textShadowOffset: { width: 0, height: 0 },
        textShadowRadius: dim ? 0 : 6,
      }}
    >
      {children}
    </Text>
  );
}

/** Caps Dynamic Type so the digits stay inside their recess at the largest sizes. */
const MAX_SCALE = 1.3;

/**
 * An amber 14-segment readout in a recessed well, with the unlit "8"
 * segments faintly behind it like the desktop's PIN display. The readout is
 * one accessible element; the ghost digits are decoration.
 */
export function LedReadout({
  value,
  digits,
  size,
  dim = false,
  compact = false,
  maxScale = MAX_SCALE,
  accessibilityLabel,
  testID,
}: {
  value: string;
  digits: number;
  size: number;
  dim?: boolean;
  /** Tighter side padding, for readouts that share a narrow column. */
  compact?: boolean;
  /** How far the digits may grow with Dynamic Type; 1 keeps a fixed-width well. */
  maxScale?: number;
  accessibilityLabel: string;
  testID?: string;
}) {
  const padding = compact ? 4 : 8;
  const digitStyle = [
    type.led,
    { fontSize: size, lineHeight: size * 1.25, textAlign: "right" as const },
  ];
  return (
    <View
      accessible
      accessibilityLabel={accessibilityLabel}
      testID={testID}
      style={[wellStyle, { paddingHorizontal: padding, paddingVertical: 4 }]}
    >
      <Text
        accessibilityElementsHidden
        importantForAccessibility="no"
        maxFontSizeMultiplier={maxScale}
        style={[...digitStyle, { color: colors.ledGhost }]}
      >
        {"8".repeat(digits)}
      </Text>
      <Text
        maxFontSizeMultiplier={maxScale}
        style={[
          ...digitStyle,
          {
            position: "absolute",
            top: 4,
            right: padding,
            color: dim ? colors.tertiaryLabel : colors.amber,
            textShadowColor: colors.amber,
            textShadowOffset: { width: 0, height: 0 },
            textShadowRadius: dim ? 0 : 6,
          },
        ]}
      >
        {value}
      </Text>
    </View>
  );
}
