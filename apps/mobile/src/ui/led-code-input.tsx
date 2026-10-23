import type { Ref } from "react";
import { TextInput, type TextInputInstance, type TextInputProps } from "react-native";
import { colors, fonts } from "@/theme";
import { wellStyle } from "./led-window";

/**
 * A numeric code typed into an LED window: amber segments over ghost 8s, one
 * per digit. The first-run pairing code and a speaker's pairing code use it.
 */
export function LedCodeInput({
  length,
  compact = false,
  ref,
  ...props
}: Omit<TextInputProps, "style"> & {
  length: number;
  compact?: boolean;
  ref?: Ref<TextInputInstance>;
}) {
  return (
    <TextInput
      ref={ref}
      placeholder={"8".repeat(length)}
      placeholderTextColor={colors.ledGhost}
      cursorColor={colors.amber}
      selectionHandleColor={colors.amber}
      maxFontSizeMultiplier={1.3}
      keyboardType="number-pad"
      inputMode="numeric"
      maxLength={length}
      {...props}
      style={[
        wellStyle,
        {
          minHeight: 66,
          paddingHorizontal: 14,
          fontFamily: fonts.led,
          color: colors.amber,
          fontSize: compact ? 24 : 27,
          letterSpacing: compact ? 6 : 9,
          textAlign: "center",
        },
      ]}
    />
  );
}
