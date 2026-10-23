import { Pressable, Text, View } from "react-native";
import { colors, fonts, layout, radius, type } from "@/theme";
import { Lamp, type LampState } from "./lamp";

export type StripTone = "idle" | "selected" | "connecting" | "live" | "failed" | "unavailable";

const tones: Record<
  StripTone,
  { lamp: LampState; surface: string; border: string; ink: string; opacity?: number }
> = {
  idle: { lamp: "off", surface: colors.faceRow, border: colors.separator, ink: colors.amber },
  selected: { lamp: "amber", surface: colors.faceRow, border: colors.key, ink: colors.amberBright },
  connecting: {
    lamp: "amber",
    surface: colors.faceRow,
    border: colors.separator,
    ink: colors.amber,
  },
  live: {
    lamp: "live",
    surface: colors.selectedSurface,
    border: colors.signLitLine,
    ink: colors.accent,
  },
  failed: {
    lamp: "error",
    surface: colors.errorSurface,
    border: colors.errorBorder,
    ink: colors.onError,
  },
  unavailable: {
    lamp: "off",
    surface: colors.faceRow,
    border: colors.separator,
    ink: colors.tertiaryLabel,
    opacity: 0.45,
  },
};

/**
 * One choice in a picker sheet, cut like a channel strip on the faceplate: a
 * lamp, the device's name over its kind, and the word for what a tap does.
 * Names wrap to two lines so the action word always shows whole.
 */
export function ChannelStrip({
  name,
  caption,
  action,
  tone,
  disabled = false,
  onPress,
  accessibilityHint,
  testID,
}: {
  name: string;
  caption: string;
  action: string;
  tone: StripTone;
  disabled?: boolean;
  onPress: () => void;
  accessibilityHint?: string;
  testID: string;
}) {
  const look = tones[tone];
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={`${name}, ${caption}`}
      accessibilityValue={{ text: action }}
      accessibilityHint={accessibilityHint}
      accessibilityState={{
        selected: tone === "selected" || tone === "live",
        busy: tone === "connecting",
        disabled,
      }}
      disabled={disabled}
      onPress={onPress}
      testID={testID}
      style={({ pressed }) => ({
        flexDirection: "row",
        alignItems: "center",
        gap: 10,
        minHeight: layout.minTarget,
        paddingVertical: 10,
        paddingHorizontal: 12,
        borderWidth: 1,
        borderRadius: radius.lg,
        borderCurve: "continuous",
        backgroundColor: pressed ? colors.surfacePressed : look.surface,
        borderColor: look.border,
        // Disabled strips dim like a disabled key; the one connecting stays bright to show its work.
        opacity: disabled && tone !== "connecting" ? 0.45 : look.opacity,
      })}
    >
      <Lamp state={look.lamp} testID={`${testID}-lamp`} />
      <View testID={`${testID}-text`} style={{ flex: 1, gap: 2 }}>
        <Text
          maxFontSizeMultiplier={1.3}
          numberOfLines={2}
          style={{
            fontFamily: fonts.sansMedium,
            fontSize: 15,
            lineHeight: 20,
            color: colors.label,
          }}
        >
          {name}
        </Text>
        <Text
          maxFontSizeMultiplier={1.3}
          numberOfLines={1}
          style={[
            type.label,
            { fontSize: 10, letterSpacing: 10 * 0.16, color: colors.tertiaryLabel },
          ]}
        >
          {caption}
        </Text>
      </View>
      <Text
        maxFontSizeMultiplier={1.3}
        numberOfLines={1}
        testID={`${testID}-action`}
        style={[type.label, { color: look.ink, flexShrink: 0 }]}
      >
        {action}
      </Text>
    </Pressable>
  );
}
