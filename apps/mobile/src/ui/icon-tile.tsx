import type { IconName } from "@expo/ui";
import { View } from "react-native";
import { colors, radius } from "@/theme";
import { NativeIcon } from "./native-icon";

const sizes = {
  md: { tile: 54, icon: 28 },
  lg: { tile: 58, icon: 30 },
} as const;

/** Decorative rounded square behind a row's leading icon. The row carries the label. */
export function IconTile({ name, size = "md" }: { name: IconName; size?: keyof typeof sizes }) {
  const { tile, icon } = sizes[size];
  return (
    <View
      accessibilityElementsHidden
      importantForAccessibility="no-hide-descendants"
      style={{
        width: tile,
        height: tile,
        borderRadius: radius.lg,
        borderCurve: "continuous",
        alignItems: "center",
        justifyContent: "center",
        backgroundColor: colors.iconSurface,
        borderWidth: 1,
        borderColor: colors.border,
      }}
    >
      <NativeIcon name={name} size={icon} />
    </View>
  );
}
