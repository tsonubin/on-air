import { fonts } from "./theme";

/**
 * The vendored OFL fonts (see assets/fonts/OFL.txt), keyed by PostScript name
 * so React Native text and SwiftUI `Font.custom` resolve the same family.
 */
export const fontAssets: Record<(typeof fonts)[keyof typeof fonts], number> = {
  [fonts.sign]: require("../assets/fonts/Anton-Regular.ttf"),
  [fonts.led]: require("../assets/fonts/DSEG14ClassicMini-Regular.ttf"),
  [fonts.mono]: require("../assets/fonts/IBMPlexMono-Regular.ttf"),
  [fonts.monoMedium]: require("../assets/fonts/IBMPlexMono-Medium.ttf"),
  [fonts.sans]: require("../assets/fonts/IBMPlexSans-Regular.ttf"),
  [fonts.sansMedium]: require("../assets/fonts/IBMPlexSans-Medium.ttf"),
  [fonts.sansSemiBold]: require("../assets/fonts/IBMPlexSans-SemiBold.ttf"),
};
