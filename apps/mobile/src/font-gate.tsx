import { useFonts } from "expo-font";
import type { ReactNode } from "react";
import { View } from "react-native";
import { fontAssets } from "./font-assets";
import { colors } from "./theme";

/**
 * Holds a plain well-colored view until the bundled fonts register. Local
 * assets load in milliseconds; if loading fails the app still renders, and
 * text falls back to the platform's system font.
 */
export function FontGate({ children }: { children: ReactNode }) {
  const [loaded, error] = useFonts(fontAssets);
  if (!loaded && !error) {
    return <View testID="font-gate" style={{ flex: 1, backgroundColor: colors.background }} />;
  }
  return children;
}
