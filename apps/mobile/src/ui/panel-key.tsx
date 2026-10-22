import type { IconName } from "@expo/ui";
import {
  ActivityIndicator,
  Pressable,
  type StyleProp,
  Text,
  View,
  type ViewStyle,
} from "react-native";
import { colors, radius, type } from "@/theme";
import { NativeIcon } from "./native-icon";

/**
 * A key's face: lit from above with a hard drop below, like the desktop's
 * hardware keys. Pressing it sinks the key and turns the bevel inward. A lit
 * key is solid amber.
 */
export function keyFace({ pressed, lit }: { pressed: boolean; lit: boolean }): ViewStyle {
  const transform = [{ translateY: pressed ? 1 : 0 }];
  if (lit) {
    return {
      backgroundColor: colors.amber,
      boxShadow: pressed
        ? "inset 0 1px 2px rgba(0,0,0,0.35)"
        : `inset 0 1px 0 rgba(255,255,255,0.25), 0 2px 0 ${colors.background}`,
      transform,
    };
  }
  return {
    backgroundColor: colors.keyBottom,
    experimental_backgroundImage: pressed
      ? `linear-gradient(180deg, ${colors.keyBottom}, ${colors.keyTop})`
      : `linear-gradient(180deg, ${colors.keyTop}, ${colors.keyBottom})`,
    boxShadow: pressed
      ? "inset 0 1px 2px rgba(0,0,0,0.5)"
      : `inset 0 1px 0 ${colors.keyHighlight}, 0 2px 0 ${colors.background}`,
    transform,
  };
}

const shapes = {
  key: { minHeight: 56, borderRadius: radius.sm },
  round: { width: 36, height: 36, borderRadius: 18 },
} satisfies Record<string, ViewStyle>;

/** A beveled faceplate key: a mono caption or an icon, an optional amber dot above. */
export function PanelKey({
  label,
  icon,
  iconSize,
  shape = "key",
  onPress,
  disabled = false,
  busy = false,
  indicator = false,
  lit = false,
  accessibilityLabel,
  accessibilityHint,
  accessibilityValue,
  style,
  testID,
}: {
  label?: string;
  icon?: IconName;
  iconSize?: number;
  shape?: "key" | "round";
  onPress: () => void;
  disabled?: boolean;
  busy?: boolean;
  indicator?: boolean;
  lit?: boolean;
  accessibilityLabel?: string;
  accessibilityHint?: string;
  accessibilityValue?: { text: string };
  style?: StyleProp<ViewStyle>;
  testID: string;
}) {
  const ink = lit ? colors.onAmber : colors.label;
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={accessibilityLabel ?? label}
      accessibilityHint={accessibilityHint}
      accessibilityValue={accessibilityValue}
      accessibilityState={{ disabled, busy }}
      disabled={disabled}
      onPress={onPress}
      hitSlop={shape === "round" ? 4 : undefined}
      testID={testID}
      style={({ pressed }) => [
        shapes[shape],
        {
          alignItems: "center",
          justifyContent: "center",
          gap: 6,
          paddingHorizontal: shape === "round" ? 0 : 8,
          borderCurve: "continuous",
        },
        keyFace({ pressed, lit }),
        // A busy key is not pressable either, but stays at full strength so its spinner shows.
        disabled && !busy && { opacity: 0.45 },
        style,
      ]}
    >
      {indicator ? (
        <View
          testID={`${testID}-indicator`}
          style={{
            position: "absolute",
            top: 10,
            width: 6,
            height: 6,
            borderRadius: 3,
            backgroundColor: colors.amber,
          }}
        />
      ) : null}
      {busy ? (
        <ActivityIndicator color={lit ? colors.onAmber : colors.amber} />
      ) : (
        <>
          {icon ? <NativeIcon name={icon} size={iconSize ?? 19} color={ink} /> : null}
          {label ? (
            <Text
              maxFontSizeMultiplier={1.3}
              numberOfLines={1}
              style={[type.label, { color: ink, textAlign: "center" }]}
            >
              {label}
            </Text>
          ) : null}
        </>
      )}
    </Pressable>
  );
}
