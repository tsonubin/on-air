import { type ReactNode, useState } from "react";
import { Image, type StyleProp, View, type ViewStyle } from "react-native";
import { colors } from "@/theme";

const grain = require("../../assets/textures/grain.png");

/** Satin anodized (finish A): a soft top highlight over a warm vertical gradient. */
const finish = `radial-gradient(ellipse 120% 45% at 50% 0%, ${colors.faceHighlight}, transparent), linear-gradient(180deg, ${colors.faceTop}, ${colors.faceBottom})`;

/** The grain tile's size in points; the @2x / @3x files keep one noise pixel per device pixel. */
const TILE = 128;

/**
 * One continuous faceplate behind a screen: the gradient, the top highlight
 * and an almost invisible grain, drawn behind the children and hidden from
 * touches and screen readers. The grain is laid out tile by tile once the
 * plate is measured; `resizeMode="repeat"` draws only a single tile here.
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
  const [size, setSize] = useState({ width: 0, height: 0 });
  const columns = Math.ceil(size.width / TILE);
  const rows = Math.ceil(size.height / TILE);
  const tiles = [];
  for (let row = 0; row < rows; row += 1) {
    for (let column = 0; column < columns; column += 1) {
      tiles.push(
        <Image
          key={`${row}:${column}`}
          source={grain}
          style={{
            position: "absolute",
            left: column * TILE,
            top: row * TILE,
            width: TILE,
            height: TILE,
          }}
        />,
      );
    }
  }
  return (
    <View testID={testID} style={[{ flex: 1, backgroundColor: colors.faceBottom }, style]}>
      <View
        testID="faceplate-finish"
        pointerEvents="none"
        accessibilityElementsHidden
        importantForAccessibility="no-hide-descendants"
        onLayout={(event) => {
          const { width, height } = event.nativeEvent.layout;
          setSize((current) =>
            current.width === width && current.height === height ? current : { width, height },
          );
        }}
        style={{
          position: "absolute",
          top: 0,
          right: 0,
          bottom: 0,
          left: 0,
          overflow: "hidden",
          backgroundColor: colors.faceBottom,
          experimental_backgroundImage: finish,
        }}
      >
        <View
          testID="faceplate-grain"
          style={{ position: "absolute", top: 0, right: 0, bottom: 0, left: 0, opacity: 0.035 }}
        >
          {tiles}
        </View>
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
