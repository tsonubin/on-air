import type { ReactNode } from "react";
import { type StyleProp, View, type ViewStyle } from "react-native";
import { colors, radius } from "@/theme";

/**
 * The faceplate every grouped block on the home screens sits on: a machined
 * panel with a hairline border and a slightly lighter top edge for the sheen.
 */
export function Card({
  children,
  style,
  testID,
  rounded = "lg",
}: {
  children: ReactNode;
  style?: StyleProp<ViewStyle>;
  testID?: string;
  rounded?: keyof typeof radius;
}) {
  return (
    <View
      testID={testID}
      style={[
        {
          overflow: "hidden",
          backgroundColor: colors.surface,
          borderRadius: radius[rounded],
          borderCurve: "continuous",
          borderWidth: 1,
          borderColor: colors.border,
          borderTopColor: colors.borderStrong,
        },
        style,
      ]}
    >
      {children}
    </View>
  );
}
