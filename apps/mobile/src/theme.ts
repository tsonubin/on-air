import type { TextStyle } from "react-native";

/**
 * The app is dark-only (`app.json` sets `userInterfaceStyle: "dark"`), and its
 * palette mirrors the desktop tray app's tokens in `apps/desktop/src/App.css`:
 * warm faceplates, cream ink, amber values and controls. Red is reserved for
 * the live state (the lit sign, the lamp, the LIVE status, the active route).
 */
export const colors = {
  background: "#0a0908",
  surface: "#1a1814",
  surfaceRaised: "#221f1a",
  surfacePressed: "#2a2218",
  iconSurface: "#2a261f",
  selectedSurface: "#3a221c",
  border: "#3a342a",
  borderStrong: "#4a4338",
  separator: "#2e2a24",
  label: "#f3ead4",
  secondaryLabel: "#c4b89a",
  tertiaryLabel: "#9a8f78",
  accent: "#ff3b2a",
  onAccent: "#ffffff",
  liveDim: "#6a2a22",
  amber: "#e0a24b",
  amberBright: "#f0c56a",
  key: "#5c4a32",
  onAmber: "#141210",
  ledWell: "#070605",
  ledGhost: "#1c1812",
  signLitText: "#ff3b2a",
  signLitWell: "#1a0605",
  signLitLine: "#8a3a30",
  signOffText: "#5a1814",
  signOffWell: "#0e0c0a",
  disabled: "#5b5246",
  errorSurface: "#3a1410",
  errorBorder: "#6a2a22",
  onError: "#f0b4ac",
} as const;

/**
 * The desktop's fonts, registered at launch under their PostScript names so
 * the same name works for React Native `fontFamily` and SwiftUI `Font.custom`.
 */
export const fonts = {
  sign: "Anton-Regular",
  led: "DSEG14ClassicMini-Regular",
  mono: "IBMPlexMono-Regular",
  monoMedium: "IBMPlexMono-Medium",
  sans: "IBMPlexSans-Regular",
  sansMedium: "IBMPlexSans-Medium",
  sansSemiBold: "IBMPlexSans-SemiBold",
} as const;

export const spacing = {
  xs: 4,
  sm: 8,
  md: 16,
  lg: 24,
  xl: 32,
} as const;

export const radius = {
  sm: 4,
  md: 6,
  lg: 8,
  xl: 10,
  xxl: 12,
} as const;

/** Type presets. Custom families carry their own weight, so none set `fontWeight`. */
export const type = {
  sign: { fontFamily: fonts.sign, fontSize: 18, letterSpacing: 2 },
  led: { fontFamily: fonts.led },
  label: {
    fontFamily: fonts.monoMedium,
    fontSize: 11,
    letterSpacing: 11 * 0.16,
    textTransform: "uppercase",
  },
  title: { fontFamily: fonts.sansSemiBold, fontSize: 17, lineHeight: 22 },
  body: { fontFamily: fonts.sans, fontSize: 15, lineHeight: 20 },
} as const satisfies Record<string, TextStyle>;

export const layout = {
  /** Widest the mixer and pairing columns grow on tablets and open foldables. */
  maxWidth: 960,
  /** Smallest comfortable touch target (pt / dp). */
  minTarget: 44,
  /** Window width at or above which the two-pane layout is used. */
  wideBreakpoint: 700,
  /** Window width below which paddings tighten. */
  compactBreakpoint: 370,
} as const;

export const theme = {
  seedColor: colors.amber,
  colors,
  fonts,
  spacing,
  radius,
  type,
  layout,
  motion: {
    controlCommitMs: 180,
    /** How long a fader ignores remote values after its last commit settles. */
    interactionSettleMs: 300,
  },
} as const;
