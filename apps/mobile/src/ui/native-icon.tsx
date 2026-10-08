import { Host, Icon, type IconName } from "@expo/ui";
import { colors } from "@/theme";

/**
 * A platform icon in its own tiny native host. Without `accessibilityLabel`
 * the icon is decorative and hidden from screen readers, which is what every
 * icon inside a labelled button wants; pass a label only for an icon that
 * carries meaning on its own.
 */
export function NativeIcon({
  name,
  size = 24,
  color = colors.label,
  accessibilityLabel,
}: {
  name: IconName;
  size?: number;
  color?: string;
  accessibilityLabel?: string;
}) {
  const decorative = accessibilityLabel === undefined;
  return (
    <Host
      matchContents
      ignoreSafeArea="all"
      colorScheme="dark"
      seedColor={colors.accent}
      style={{ width: size, height: size }}
      accessible={!decorative}
      accessibilityRole={decorative ? undefined : "image"}
      accessibilityLabel={accessibilityLabel}
      accessibilityElementsHidden={decorative}
      importantForAccessibility={decorative ? "no-hide-descendants" : "yes"}
    >
      <Icon name={name} size={size} color={color} accessibilityLabel={accessibilityLabel} />
    </Host>
  );
}
