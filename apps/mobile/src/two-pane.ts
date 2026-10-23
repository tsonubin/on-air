import { layout } from "./theme";

/**
 * Whether a window is roomy enough to show the mixer and the equalizer side by
 * side: an unfolded iPhone Duo (about 669 × 951 pt upright), iPads and Android
 * foldables. Landscape phones are wide but too short for the faders, and keep
 * their own layout. Read from the window's current size, so folding, unfolding
 * and split view switch it live.
 */
export function isTwoPane(width: number, height: number): boolean {
  return width >= layout.twoPaneMinWidth && height >= layout.twoPaneMinHeight;
}
