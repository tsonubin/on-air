import { Text, View } from "react-native";
import type { Readout } from "./readouts";
import { colors, type } from "./theme";
import { Lamp } from "./ui/lamp";
import { LedText } from "./ui/led-readout";
import { LedWindow } from "./ui/led-window";
import type { HomeStatus } from "./ui-state";

/** A silkscreened caption printed inside the window. */
function Caption({ children }: { children: string }) {
  return (
    <Text maxFontSizeMultiplier={1.3} style={[type.label, { color: colors.tertiaryLabel }]}>
      {children}
    </Text>
  );
}

/**
 * The mixer's recessed LED window: what is streaming, where it plays, the
 * status word with its lamp, and the format. It reads as one summary; the
 * captions and segments inside are not separate stops for a screen reader.
 */
export function LedDisplay({
  source,
  speaker,
  status,
  rate,
}: {
  /** The source name, or "" when none is chosen. */
  source: string;
  /** The speaker name, or "" when none is chosen. */
  speaker: string;
  status: HomeStatus;
  rate: Readout;
}) {
  const spokenRate = rate.spoken.charAt(0).toUpperCase() + rate.spoken.slice(1);
  const summary = [
    status.announcement,
    source ? `Source ${source}` : "No source chosen",
    speaker ? `Speaker ${speaker}` : "No speaker chosen",
    spokenRate,
  ].join(". ");
  return (
    <LedWindow
      testID="led-window"
      accessible
      accessibilityRole="text"
      accessibilityLabel={`${summary}.`}
      style={{ gap: 10 }}
    >
      <View style={{ gap: 4 }}>
        <Caption>Source</Caption>
        <LedText size={18} dim={!source} testID="led-source">
          {source || "Choose a source"}
        </LedText>
      </View>
      <View style={{ gap: 4 }}>
        <Caption>Speaker</Caption>
        <LedText size={18} dim={!speaker} testID="led-speaker">
          {speaker || "Choose a speaker"}
        </LedText>
      </View>
      <View style={{ flexDirection: "row", alignItems: "center", gap: 8 }}>
        <Lamp state={status.live ? "live" : "off"} size={8} testID="live-lamp" />
        <LedText size={14} testID="led-status">
          {status.word}
        </LedText>
        <View style={{ flex: 1 }} />
        <Text
          testID="led-format"
          maxFontSizeMultiplier={1.3}
          style={[type.label, { color: colors.tertiaryLabel }]}
        >
          {`${rate.text} · ST`}
        </Text>
      </View>
    </LedWindow>
  );
}
