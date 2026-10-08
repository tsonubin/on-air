import type { ReactNode } from "react";
import { type StyleProp, View, type ViewStyle } from "react-native";
import { colors, radius } from "@/theme";

/** The rounded, bordered surface every grouped block on the home screens sits on. */
export function Card({
  children,
  style,
  testID,
  rounded = "xl",
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
        },
        style,
      ]}
    >
      {children}
    </View>
  );
}
