import type { ReactNode } from "react";
import { Image, type StyleProp, View, type ViewStyle } from "react-native";
import { colors } from "@/theme";

const grain = require("../../assets/textures/grain.png");

/** Satin anodized (finish A): a soft top highlight over a warm vertical gradient. */
const finish = `radial-gradient(ellipse 120% 45% at 50% 0%, ${colors.faceHighlight}, transparent), linear-gradient(180deg, ${colors.faceTop}, ${colors.faceBottom})`;

/**
 * One continuous faceplate behind a screen: the gradient, the top highlight
 * and an almost invisible grain tile, drawn behind the children and hidden
 * from touches and screen readers.
 */
export function Faceplate({
  children,
  style,
  testID,
}: {
  children?: ReactNode;
  style?: StyleProp<ViewStyle>;
  testID?: string;
}) {
  return (
    <View testID={testID} style={[{ flex: 1, backgroundColor: colors.faceBottom }, style]}>
      <View
        testID="faceplate-finish"
        pointerEvents="none"
        accessibilityElementsHidden
        importantForAccessibility="no-hide-descendants"
        style={{
          position: "absolute",
          top: 0,
          right: 0,
          bottom: 0,
          left: 0,
          backgroundColor: colors.faceBottom,
          experimental_backgroundImage: finish,
        }}
      >
        <Image
          source={grain}
          resizeMode="repeat"
          style={{ position: "absolute", top: 0, right: 0, bottom: 0, left: 0, opacity: 0.035 }}
        />
      </View>
      {children}
    </View>
  );
}

/** An engraved line between groups on the faceplate. */
export function Groove({ testID }: { testID?: string }) {
  return (
    <View
      testID={testID}
      accessibilityElementsHidden
      importantForAccessibility="no-hide-descendants"
      style={{
        alignSelf: "stretch",
        borderTopWidth: 1,
        borderTopColor: colors.grooveDark,
        borderBottomWidth: 1,
        borderBottomColor: colors.grooveLight,
      }}
    />
  );
}
