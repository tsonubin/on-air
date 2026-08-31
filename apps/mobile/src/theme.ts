export const theme = {
  // Warm broadcast red keeps the product identity while Host derives native
  // SwiftUI and Material 3 control colors around it.
  seedColor: "#c84735",
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
