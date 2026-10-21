// Expo Router's Stack reads its own theme context, so the base theme comes from expo-router.
import { DarkTheme } from "expo-router";
import { colors, fonts } from "./theme";

/** Native stack chrome in the faceplate palette: warm header, cream title, amber back button. */
export const navigationTheme = {
  ...DarkTheme,
  colors: {
    ...DarkTheme.colors,
    background: colors.background,
    card: colors.surface,
    text: colors.label,
    border: colors.separator,
    primary: colors.amber,
    notification: colors.accent,
  },
};

/**
 * A screen's header shows its title as an uppercase caption; `title` stays in
 * plain case because it also labels the next screen's back button.
 */
export function titledScreen(title: string) {
  return { title, headerTitle: title.toUpperCase() };
}

export const screenHeaderOptions = {
  headerTintColor: colors.amber,
  headerTitleStyle: { fontFamily: fonts.monoMedium, color: colors.label },
};
