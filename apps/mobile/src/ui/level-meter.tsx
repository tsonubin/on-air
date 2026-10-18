import type { MeterLevel } from "@on-air/api-types";
import { levelFraction, type StereoLevels } from "@on-air/control-client";
import { useEffect, useState } from "react";
import { Text, View } from "react-native";
import { colors } from "@/theme";

/** Silence for this long reads zero, like the TUI's meter timeout. */
const QUIET_MS = 1_000;

const SILENT: StereoLevels = {
  left: { rms: 0, peak: 0 },
  right: { rms: 0, peak: 0 },
};

function ChannelBar({
  channel,
  level,
  disabled,
}: {
  channel: "L" | "R";
  level: MeterLevel;
  disabled: boolean;
}) {
  const rms = levelFraction(level.rms);
  const peak = levelFraction(level.peak);
  return (
    <View style={{ flexDirection: "row", alignItems: "center", gap: 8 }}>
      <Text style={{ width: 12, color: colors.tertiaryLabel, fontSize: 12 }}>{channel}</Text>
      <View
        accessible
        accessibilityLabel={`${channel === "L" ? "Left" : "Right"} channel level`}
        accessibilityRole="progressbar"
        accessibilityValue={{ min: 0, max: 100, now: Math.round(rms * 100) }}
        style={{
          flex: 1,
          height: 8,
          borderRadius: 4,
          backgroundColor: colors.surfacePressed,
          overflow: "hidden",
          opacity: disabled ? 0.4 : 1,
        }}
      >
        <View
          style={{
            position: "absolute",
            left: 0,
            top: 0,
            bottom: 0,
            width: `${rms * 100}%`,
            backgroundColor: colors.accent,
          }}
        />
        <View
          style={{
            position: "absolute",
            left: `${peak * 100}%`,
            top: 0,
            bottom: 0,
            width: 2,
            backgroundColor: colors.label,
          }}
        />
      </View>
    </View>
  );
}

/**
 * The stereo level meter: one RMS bar with a peak tick per channel, fed by
 * `LevelMeter` frames. When frames stop arriving the bars read zero after a
 * second instead of freezing at their last values.
 */
export function LevelMeter({
  levels,
  disabled,
}: {
  levels: StereoLevels | null;
  disabled: boolean;
}): React.JSX.Element {
  const [fresh, setFresh] = useState(false);
  useEffect(() => {
    if (!levels) return;
    setFresh(true);
    const id = setTimeout(() => setFresh(false), QUIET_MS);
    return () => clearTimeout(id);
  }, [levels]);
  const shown = fresh && levels ? levels : SILENT;
  return (
    <View style={{ gap: 6 }} testID="level-meter">
      <Text style={{ color: colors.secondaryLabel, fontSize: 13 }}>Level</Text>
      <ChannelBar channel="L" level={shown.left} disabled={disabled} />
      <ChannelBar channel="R" level={shown.right} disabled={disabled} />
    </View>
  );
}
