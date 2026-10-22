import { useState } from "react";
import { Text, View } from "react-native";
import { formatDb } from "./readouts";
import { colors, type } from "./theme";
import { Fader } from "./ui/fader";
import { LedReadout, LedText } from "./ui/led-readout";
import { LedWindow } from "./ui/led-window";

/** The five bands, low to high: what each fader prints and what it says aloud. */
export const bands = [
  { freq: "60", name: "Bass", spoken: "Bass, 60 hertz" },
  { freq: "250", name: "Warmth", spoken: "Warmth, 250 hertz" },
  { freq: "1K", name: "Mid", spoken: "Midrange, 1 kilohertz" },
  { freq: "4K", name: "Pres", spoken: "Presence, 4 kilohertz" },
  { freq: "12K", name: "Air", spoken: "Air, 12 kilohertz" },
] as const;

/** Height a column spends on its dB readout, labels and gaps around the track. */
const COLUMN_CHROME = 96;
/** The dB readout and its gap above each track, so the scale and 0 dB line line up. */
const READOUT_HEIGHT = 36;

/** The fader track fills its row, within a usable range on short and tall screens. */
export function eqTrackLength(rowHeight: number): number {
  return Math.min(320, Math.max(140, rowHeight - COLUMN_CHROME));
}

const hidden = {
  accessibilityElementsHidden: true,
  importantForAccessibility: "no-hide-descendants",
} as const;

/**
 * The equalizer as five vertical faders on the faceplate: an LED dB readout
 * over each slot, a printed ±12 dB scale and a 0 dB line across them, and a
 * FLAT / CUSTOM curve readout above. Each fader speaks its band and value; the
 * printed parts are decoration.
 */
export function EqPanel({
  gains,
  disabled,
  onChangeBand,
}: {
  gains: number[];
  disabled: boolean;
  onChangeBand: (index: number, value: number) => void;
}) {
  const [rowHeight, setRowHeight] = useState(0);
  const length = eqTrackLength(rowHeight);
  const flat = gains.every((gain) => gain === 0);
  const caption = [type.label, { color: colors.tertiaryLabel }];

  return (
    <View style={{ flex: 1, gap: 18 }}>
      <LedWindow
        style={{ flexDirection: "row", alignItems: "center", justifyContent: "space-between" }}
      >
        <Text style={caption}>Curve</Text>
        <LedText size={18} testID="eq-curve">
          {flat ? "FLAT" : "CUSTOM"}
        </LedText>
      </LedWindow>

      <View
        style={{ flex: 1, flexDirection: "row" }}
        onLayout={(event) => setRowHeight(event.nativeEvent.layout.height)}
      >
        {/* The scale column also keeps the first fader clear of iOS's back-swipe edge. */}
        <View
          testID="eq-scale"
          {...hidden}
          style={{
            width: 30,
            marginTop: READOUT_HEIGHT - 8,
            height: length + 16,
            justifyContent: "space-between",
          }}
        >
          {["+12", "+6", "0", "−6", "−12"].map((mark) => (
            <Text key={mark} style={caption}>
              {mark}
            </Text>
          ))}
        </View>

        <View style={{ flex: 1, flexDirection: "row" }}>
          <View
            testID="eq-zero-line"
            {...hidden}
            pointerEvents="none"
            style={{
              position: "absolute",
              left: 0,
              right: 0,
              top: READOUT_HEIGHT + length / 2,
              height: 1,
              backgroundColor: colors.grooveLight,
            }}
          />
          {bands.map((band, index) => (
            <View key={band.freq} style={{ flex: 1, alignItems: "center", gap: 6 }}>
              <Fader
                orientation="vertical"
                value={gains[index] ?? 0}
                min={-12}
                max={12}
                step={0.5}
                origin={0}
                length={length}
                accessibilityStep={1}
                accessibilityLabel={band.spoken}
                formatValue={(value) => formatDb(value).spoken}
                renderReadout={(draft) => (
                  <View {...hidden}>
                    <LedReadout
                      value={formatDb(draft).text}
                      digits={4}
                      size={14}
                      dim={disabled}
                      accessibilityLabel=""
                      testID={`eq-band-${index}-readout`}
                    />
                  </View>
                )}
                disabled={disabled}
                testID={`eq-band-${index}`}
                onChange={(value) => onChangeBand(index, value)}
              />
              <View {...hidden} style={{ alignItems: "center", gap: 2 }}>
                <Text maxFontSizeMultiplier={1.3} style={[type.label, { color: colors.label }]}>
                  {band.freq}
                </Text>
                <Text
                  maxFontSizeMultiplier={1.3}
                  style={[
                    type.body,
                    { fontSize: 12, lineHeight: 16, color: colors.secondaryLabel },
                  ]}
                >
                  {band.name}
                </Text>
              </View>
            </View>
          ))}
        </View>
      </View>

      <Text style={[type.body, { fontSize: 13, lineHeight: 18, color: colors.secondaryLabel }]}>
        Drag up to boost, down to cut. 0 dB leaves the sound unchanged.
      </Text>
    </View>
  );
}
