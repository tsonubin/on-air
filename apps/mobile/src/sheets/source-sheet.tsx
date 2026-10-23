import { ScrollView, Text, View } from "react-native";
import { useRemoteSession } from "@/remote-session";
import { colors, type } from "@/theme";
import { ChannelStrip } from "@/ui/channel-strip";
import { Groove } from "@/ui/faceplate";
import { Lamp } from "@/ui/lamp";
import { LedText } from "@/ui/led-readout";
import { LedWindow } from "@/ui/led-window";
import { SheetPanel } from "./sheet-panel";

/** The source picker: the chosen capture device in the LED window, one strip per device. */
export function SourceSheet({ onClose }: { onClose: () => void }) {
  const { inputs, activeInput, activeInputLabel, busyTarget, view, pickInput } = useRemoteSession();
  const disabled = !view.controlsEnabled || busyTarget !== null;
  return (
    <SheetPanel
      spec={{ kind: "source", count: inputs.length }}
      caption="Source"
      onClose={onClose}
      testID="source-sheet"
    >
      <LedWindow
        accessible
        accessibilityRole="text"
        accessibilityLabel={activeInputLabel ? `Source ${activeInputLabel}` : "No source chosen"}
        style={{ flexDirection: "row", alignItems: "center", gap: 10 }}
      >
        <Lamp state={activeInputLabel ? "amber" : "off"} />
        <View style={{ flex: 1 }}>
          <LedText size={18} dim={!activeInputLabel} testID="source-led">
            {activeInputLabel || "No source"}
          </LedText>
        </View>
      </LedWindow>
      <Groove />
      <ScrollView contentContainerStyle={{ gap: 8 }}>
        {inputs.map(({ name, label }) => {
          const selected = name === activeInput;
          const switching = busyTarget === `input:${name}`;
          return (
            <ChannelStrip
              key={name}
              name={label}
              caption="Capture device"
              action={switching ? "Switching…" : selected ? "Selected" : "Select"}
              tone={switching ? "connecting" : selected ? "selected" : "idle"}
              disabled={disabled}
              onPress={async () => {
                if (selected) onClose();
                else if (await pickInput(name)) onClose();
              }}
              testID={`input-${name}`}
            />
          );
        })}
      </ScrollView>
      <Text
        maxFontSizeMultiplier={1.3}
        style={[type.body, { fontSize: 12, lineHeight: 16, color: colors.tertiaryLabel }]}
      >
        {inputs.length === 0
          ? "No capture devices are available."
          : "The Mac captures this device and streams it to the speaker."}
      </Text>
    </SheetPanel>
  );
}
