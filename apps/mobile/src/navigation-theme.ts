// Expo Router's Stack reads its own theme context, so the base theme comes from expo-router.
import { DarkTheme } from "expo-router";
import { colors, fonts } from "./theme";

/** Native stack chrome in the faceplate palette: warm header, cream title, amber back button. */
export const navigationTheme = {
  ...DarkTheme,
  colors: {
    ...DarkTheme.colors,
    background: colors.background,
    // The header starts where the faceplate gradient does, so the two read as one plate.
    card: colors.faceTop,
    text: colors.label,
    border: colors.separator,
    primary: colors.amber,
    notification: colors.accent,
  },
};

/**
 * A screen's header shows its title as an uppercase caption. Every screen is
 * pushed straight from the Mixer, whose plain `title` iOS uses as the back label.
 */
export function titledScreen(title: string) {
  return { title, headerTitle: title.toUpperCase() };
}

/** The pushed screens, in stack order. Mixer (index) has no header. */
export const stackScreens = [
  { name: "equalizer", options: titledScreen("Equalizer") },
  { name: "audio-format", options: titledScreen("Audio format") },
  { name: "connection", options: titledScreen("Connection") },
] as const;

export const screenHeaderOptions = {
  headerTintColor: colors.amber,
  headerTitleStyle: { fontFamily: fonts.monoMedium, color: colors.label },
  headerShadowVisible: false,
  // On iOS 26+ (SDK 58) the native bar is transparent Liquid Glass and screens draw
  // under it; an opaque faceplate-colored bar keeps every screen's content below it.
  headerTransparent: false,
  headerStyle: { backgroundColor: colors.faceTop },
};
