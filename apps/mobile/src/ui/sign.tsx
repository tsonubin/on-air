import { Text, View } from "react-native";
import { colors, radius, type } from "@/theme";

/** Caps Dynamic Type so the sign keeps to its header row at the largest sizes. */
const MAX_SCALE = 1.3;

/**
 * The ONAIR wordmark, after the desktop's illuminated sign: lit red while
 * audio is live, dark and unlit otherwise. Screen readers hear "on-air".
 */
export function Sign({ lit, testID }: { lit: boolean; testID?: string }) {
  return (
    <View
      accessible
      accessibilityRole="header"
      accessibilityLabel="on-air"
      testID={testID}
      style={{
        alignSelf: "flex-start",
        paddingHorizontal: 8,
        paddingVertical: 1,
        borderRadius: radius.sm,
        borderWidth: 1,
        backgroundColor: lit ? colors.signLitWell : colors.signOffWell,
        borderColor: lit ? colors.signLitLine : colors.separator,
      }}
    >
      <Text
        maxFontSizeMultiplier={MAX_SCALE}
        style={[
          type.sign,
          {
            color: lit ? colors.signLitText : colors.signOffText,
            textShadowColor: colors.signLitText,
            textShadowOffset: { width: 0, height: 0 },
            textShadowRadius: lit ? 8 : 0,
          },
        ]}
      >
        ONAIR
      </Text>
    </View>
  );
}
