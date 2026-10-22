import { Text, View } from "react-native";
import { colors, type } from "@/theme";

/** An inline alert printed on the faceplate under the LED window: soft red text, no box. */
export function Notice({ message, testID }: { message: string; testID?: string }) {
  return (
    <View testID={testID} style={{ paddingHorizontal: 2 }}>
      <Text accessibilityRole="alert" style={[type.body, { color: colors.onError }]}>
        {message}
      </Text>
    </View>
  );
}
