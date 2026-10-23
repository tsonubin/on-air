import type { OutputInfo, OutputPhase } from "@on-air/api-types";
import { outputPhase, TRANSPORT_LABELS } from "@on-air/api-types";
import { ScrollView, Text, View } from "react-native";
import { useRemoteSession } from "@/remote-session";
import { colors, type } from "@/theme";
import { ChannelStrip, type StripTone } from "@/ui/channel-strip";
import { Groove } from "@/ui/faceplate";
import { Lamp, type LampState } from "@/ui/lamp";
import { LedText } from "@/ui/led-readout";
import { LedWindow } from "@/ui/led-window";
import { PanelKey } from "@/ui/panel-key";
import { SheetPanel } from "./sheet-panel";

/** The trailing word on a speaker row. */
export function outputAction({
  desktopOnly,
  working,
  selected,
  phase,
  output,
}: {
  desktopOnly: boolean;
  working: boolean;
  /** This row is the desktop's active output. */
  selected: boolean;
  /** Phase of the active output; only meaningful when `selected`. */
  phase: OutputPhase | null;
  output: OutputInfo;
}): string {
  if (desktopOnly) return "Desktop only";
  if (working) return "Connecting…";
  if (selected && phase === "starting") return "Connecting…";
  if (selected && phase === "failed") return "Failed · Retry";
  if (selected) return "Connected";
  if (output.needs_pair && !output.paired) return "Pair";
  return "Connect";
}

/** How a speaker row looks: the first matching condition wins. */
export function outputTone({
  desktopOnly,
  working,
  selected,
  phase,
}: {
  desktopOnly: boolean;
  working: boolean;
  selected: boolean;
  phase: OutputPhase | null;
}): StripTone {
  if (desktopOnly) return "unavailable";
  if (working || (selected && phase === "starting")) return "connecting";
  if (selected && phase === "failed") return "failed";
  if (selected && phase === "live") return "live";
  return "idle";
}

/** The LED window's lamp, state word and spoken suffix for the active speaker's phase. */
const phaseLook: Record<OutputPhase, { lamp: LampState; word: string; ink: string }> = {
  live: { lamp: "live", word: "Live", ink: colors.accent },
  starting: { lamp: "amber", word: "Connecting…", ink: colors.amber },
  failed: { lamp: "error", word: "Failed", ink: colors.onError },
};

const footnote = [type.body, { fontSize: 12, lineHeight: 16, color: colors.tertiaryLabel }];

export function OutputSheet({
  onClose,
  onPairRequest,
}: {
  onClose: () => void;
  /** The speaker needs a PIN or confirmation before it can play. */
  onPairRequest: (output: OutputInfo) => void;
}) {
  const { outputs, activeOutput, airplayMode, busyTarget, view, activate, openBluetoothSettings } =
    useRemoteSession();

  const choose = async (output: OutputInfo) => {
    if (output.needs_pair && !output.paired) {
      onPairRequest(output);
      return;
    }
    if (await activate(output)) onClose();
  };

  const look = activeOutput ? phaseLook[outputPhase(activeOutput)] : null;
  const scanning = outputs.length === 0 && !activeOutput;
  const shown = activeOutput?.device_name ?? (scanning ? "Scanning" : "No speaker");
  const spoken = activeOutput
    ? `Speaker ${activeOutput.device_name}, ${look?.word.replace("…", "").toLowerCase()}`
    : scanning
      ? "Scanning for speakers"
      : "No speaker chosen";

  return (
    <SheetPanel
      spec={{ kind: "output", count: outputs.length }}
      caption="Speaker"
      onClose={onClose}
      testID="output-sheet"
    >
      <LedWindow
        accessible
        accessibilityRole="text"
        accessibilityLabel={spoken}
        style={{ flexDirection: "row", alignItems: "center", gap: 10 }}
      >
        <Lamp state={look?.lamp ?? "off"} />
        <View style={{ flex: 1 }}>
          <LedText size={18} dim={!activeOutput} testID="speaker-led">
            {shown}
          </LedText>
        </View>
        {look ? (
          <Text
            maxFontSizeMultiplier={1.3}
            testID="speaker-led-state"
            style={[type.label, { color: look.ink }]}
          >
            {look.word}
          </Text>
        ) : null}
      </LedWindow>
      <Groove />
      <ScrollView contentContainerStyle={{ gap: 8 }}>
        {outputs.map((output) => {
          const selected =
            activeOutput?.transport === output.transport && activeOutput.device_id === output.id;
          const phase = selected && activeOutput ? outputPhase(activeOutput) : null;
          const desktopOnly = output.transport === "airplay" && airplayMode === "avroute-picker";
          const working = busyTarget === `${output.transport}:${output.id}`;
          const stereoPair = output.member_count >= 2 || output.kind === "pair";
          return (
            <ChannelStrip
              key={`${output.transport}-${output.id}`}
              name={output.name}
              caption={`${stereoPair ? "Stereo pair · " : ""}${TRANSPORT_LABELS[output.transport]}`}
              action={outputAction({ desktopOnly, working, selected, phase, output })}
              tone={outputTone({ desktopOnly, working, selected, phase })}
              disabled={!view.controlsEnabled || busyTarget !== null || desktopOnly}
              onPress={() => void choose(output)}
              testID={`output-${output.transport}-${output.id}`}
            />
          );
        })}
      </ScrollView>
      <PanelKey
        label="+ Add Bluetooth speaker"
        accessibilityLabel="Add Bluetooth speaker"
        disabled={!view.controlsEnabled}
        onPress={() => void openBluetoothSettings()}
        testID="add-bluetooth"
        style={{ minHeight: 44 }}
      />
      {outputs.length === 0 ? (
        <Text maxFontSizeMultiplier={1.3} style={footnote}>
          Waiting for speakers on the LAN…
        </Text>
      ) : null}
      {airplayMode === "avroute-picker" ? (
        <Text maxFontSizeMultiplier={1.3} style={footnote}>
          AirPlay speakers are picked from the desktop.
        </Text>
      ) : null}
    </SheetPanel>
  );
}
