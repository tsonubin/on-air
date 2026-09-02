export const theme = {
  // System red keeps the live-state identity legible in native dark controls.
  seedColor: "#ff453a",
  spacing: {
    xs: 4,
    sm: 8,
    md: 16,
    lg: 24,
    xl: 32,
  },
  motion: {
    controlCommitMs: 180,
  },
} as const;

export const brandColors = {
  light: { error: "#b42318" },
  dark: { error: "#ffb4ab" },
} as const;
