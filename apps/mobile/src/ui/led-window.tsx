import { View, type ViewProps, type ViewStyle } from "react-native";
import { colors, radius } from "@/theme";

/**
 * A recess pressed into the faceplate: an inset shadow and a faint lit lower
 * edge, never a border. LED windows, fader slots and text fields use it.
 */
export const wellStyle: ViewStyle = {
  backgroundColor: colors.ledWell,
  borderRadius: radius.md,
  borderCurve: "continuous",
  boxShadow: `inset 0 2px 6px rgba(0,0,0,0.7), 0 1px 0 ${colors.grooveLight}`,
};

/** A recessed window for LED text and readouts. */
export function LedWindow({ style, ...props }: ViewProps) {
  return (
    <View {...props} style={[wellStyle, { paddingVertical: 12, paddingHorizontal: 14 }, style]} />
  );
}
