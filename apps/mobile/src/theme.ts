/**
 * The app is dark-only (`app.json` sets `userInterfaceStyle: "dark"`), so the
 * palette is a single set of tokens. System red keeps the live-state identity
 * legible inside native dark controls.
 */
export const colors = {
  background: "#000000",
  surface: "#171719",
  surfacePressed: "#202023",
  iconSurface: "#232326",
  selectedSurface: "#2a1515",
  border: "#343438",
  separator: "#2b2b2f",
  label: "#f5f5f7",
  secondaryLabel: "#a1a1a7",
  tertiaryLabel: "#727278",
  accent: "#ff453a",
  onAccent: "#ffffff",
  disabled: "#5b5b61",
  errorSurface: "#2b1213",
  errorBorder: "#68201f",
  onError: "#ffb4ab",
} as const;

export const spacing = {
  xs: 4,
  sm: 8,
  md: 16,
  lg: 24,
  xl: 32,
} as const;

export const radius = {
  sm: 12,
  md: 14,
  lg: 17,
  xl: 20,
  xxl: 22,
} as const;

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
  seedColor: colors.accent,
  colors,
  spacing,
  radius,
  layout,
  motion: {
    controlCommitMs: 180,
    /** How long a fader ignores remote values after its last commit settles. */
    interactionSettleMs: 300,
  },
} as const;
