import { Text, View } from "react-native";
import { colors, radius } from "@/theme";

/** An inline alert on the error surface. */
export function Notice({ message, testID }: { message: string; testID?: string }) {
  return (
    <View
      testID={testID}
      style={{
        paddingHorizontal: 14,
        paddingVertical: 12,
        borderRadius: radius.md,
        borderCurve: "continuous",
        backgroundColor: colors.errorSurface,
        borderWidth: 1,
        borderColor: colors.errorBorder,
      }}
    >
      <Text
        accessibilityRole="alert"
        style={{ color: colors.onError, fontSize: 15, lineHeight: 20 }}
      >
        {message}
      </Text>
    </View>
  );
}
