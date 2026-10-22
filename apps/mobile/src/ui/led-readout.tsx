import { Text, View } from "react-native";
import { colors, type } from "@/theme";
import { wellStyle } from "./led-window";

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
  accessibilityLabel,
  testID,
}: {
  value: string;
  digits: number;
  size: number;
  dim?: boolean;
  accessibilityLabel: string;
  testID?: string;
}) {
  const digitStyle = [
    type.led,
    { fontSize: size, lineHeight: size * 1.25, textAlign: "right" as const },
  ];
  return (
    <View
      accessible
      accessibilityLabel={accessibilityLabel}
      testID={testID}
      style={[wellStyle, { paddingHorizontal: 8, paddingVertical: 4 }]}
    >
      <Text
        accessibilityElementsHidden
        importantForAccessibility="no"
        maxFontSizeMultiplier={MAX_SCALE}
        style={[...digitStyle, { color: colors.ledGhost }]}
      >
        {"8".repeat(digits)}
      </Text>
      <Text
        maxFontSizeMultiplier={MAX_SCALE}
        style={[
          ...digitStyle,
          {
            position: "absolute",
            top: 4,
            right: 8,
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
