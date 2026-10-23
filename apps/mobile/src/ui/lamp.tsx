import { View } from "react-native";
import { colors } from "@/theme";

export type LampState = "off" | "amber" | "live" | "error";

const lampColor: Record<LampState, string> = {
  off: colors.liveDim,
  amber: colors.amber,
  live: colors.accent,
  error: colors.onError,
};

/**
 * A round indicator lamp set into the faceplate. Amber and live glow; red is
 * only ever the live lamp. Decorative: the row or window around it says the state.
 */
export function Lamp({
  state,
  size = 7,
  testID,
}: {
  state: LampState;
  size?: number;
  testID?: string;
}) {
  const color = lampColor[state];
  return (
    <View
      testID={testID}
      accessibilityElementsHidden
      importantForAccessibility="no-hide-descendants"
      style={{
        width: size,
        height: size,
        borderRadius: size / 2,
        backgroundColor: color,
        shadowColor: color,
        shadowOffset: { width: 0, height: 0 },
        shadowRadius: 6,
        shadowOpacity: state === "amber" || state === "live" ? 0.6 : 0,
      }}
    />
  );
}
