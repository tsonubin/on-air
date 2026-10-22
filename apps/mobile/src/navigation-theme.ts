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
 * A screen's header shows its title as an uppercase caption. iOS builds a back
 * button's label from the previous screen's `headerTitle`, so a screen pushed
 * from another titled screen names its back button in plain case.
 */
export function titledScreen(title: string, backTitle?: string) {
  return {
    title,
    headerTitle: title.toUpperCase(),
    ...(backTitle ? { headerBackTitle: backTitle } : {}),
  };
}

/** The pushed screens, in stack order. Mixer (index) has no header. */
export const stackScreens = [
  { name: "sound", options: titledScreen("Sound") },
  { name: "equalizer", options: titledScreen("Equalizer", "Sound") },
  { name: "audio-format", options: titledScreen("Audio format", "Sound") },
  { name: "connection", options: titledScreen("Connection") },
] as const;

export const screenHeaderOptions = {
  headerTintColor: colors.amber,
  headerTitleStyle: { fontFamily: fonts.monoMedium, color: colors.label },
  headerShadowVisible: false,
};
