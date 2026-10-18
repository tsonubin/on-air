import { BottomSheet, Button, Column, Row, Spacer, Text } from "@expo/ui";
import type { ChannelMode, OutputInfo, OutputPhase } from "@on-air/api-types";
import { CHANNEL_MODE_LABELS, outputPhase, TRANSPORT_LABELS } from "@on-air/api-types";
import { useRemoteSession } from "@/remote-session";
import { theme } from "@/theme";
import { sheetProps } from "./sheet-props";

/** The modes a speaker offers, in the order the control lists them. */
const CHANNEL_MODES: readonly ChannelMode[] = ["auto", "stereo", "mono"];

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

export function OutputSheet({
  onClose,
  onPairRequest,
}: {
  onClose: () => void;
  /** The speaker needs a PIN or confirmation before it can play. */
  onPairRequest: (output: OutputInfo) => void;
}) {
  const {
    outputs,
    activeOutput,
    airplayMode,
    busyTarget,
    view,
    activate,
    openBluetoothSettings,
    applyChannelMode,
  } = useRemoteSession();

  const choose = async (output: OutputInfo) => {
    if (output.needs_pair && !output.paired) {
      onPairRequest(output);
      return;
    }
    if (await activate(output)) onClose();
  };

  return (
    <BottomSheet
      {...sheetProps({ kind: "output", count: outputs.length })}
      isPresented
      onDismiss={onClose}
      testID="output-sheet"
    >
      <Column spacing={theme.spacing.md} style={{ padding: theme.spacing.md }}>
        <Row alignment="center" spacing={theme.spacing.sm}>
          <Text textStyle={{ fontSize: 22, fontWeight: "700" }}>Choose speaker</Text>
          <Spacer />
          <Button label="Done" variant="text" onPress={onClose} />
        </Row>
        <Text>Auto fits the speaker, Stereo never downmixes, and Mono mixes left and right.</Text>
        {outputs.length === 0 && <Text>Waiting for speakers on the LAN…</Text>}
        {airplayMode === "avroute-picker" && (
          <Text>AirPlay selection is available from the desktop picker on macOS.</Text>
        )}
        <Button
          label="Add Bluetooth speaker"
          variant="outlined"
          disabled={!view.controlsEnabled}
          onPress={() => void openBluetoothSettings()}
          testID="add-bluetooth"
        />
        {outputs.map((output) => {
          const selected =
            activeOutput?.transport === output.transport && activeOutput.device_id === output.id;
          const phase = selected && activeOutput ? outputPhase(activeOutput) : null;
          const desktopOnly = output.transport === "airplay" && airplayMode === "avroute-picker";
          const working = busyTarget === `${output.transport}:${output.id}`;
          const stereoPair = output.member_count >= 2 || output.kind === "pair";
          // An absent saved mode is `auto`, the historical fit.
          const mode = output.channel_mode ?? "auto";
          return (
            <Column key={`${output.transport}-${output.id}`} spacing={theme.spacing.xs}>
              <Button
                variant={selected && phase === "live" ? "filled" : "outlined"}
                disabled={!view.controlsEnabled || busyTarget !== null || desktopOnly}
                onPress={() => void choose(output)}
                testID={`output-${output.transport}-${output.id}`}
              >
                <Row alignment="center" spacing={theme.spacing.sm}>
                  <Column spacing={theme.spacing.xs}>
                    <Text textStyle={{ fontWeight: "600" }}>{output.name}</Text>
                    <Text testID={`channel-mode-${output.transport}-${output.id}-badge`}>{`${
                      stereoPair ? "Stereo pair · " : ""
                    }${TRANSPORT_LABELS[output.transport]} · ${CHANNEL_MODE_LABELS[mode]}`}</Text>
                  </Column>
                  <Spacer />
                  <Text>{outputAction({ desktopOnly, working, selected, phase, output })}</Text>
                </Row>
              </Button>
              <Row alignment="center" spacing={theme.spacing.sm}>
                <Text>Channel</Text>
                <Spacer />
                <Row spacing={theme.spacing.xs}>
                  {CHANNEL_MODES.map((pick) => (
                    <Button
                      key={pick}
                      label={CHANNEL_MODE_LABELS[pick]}
                      variant={pick === mode ? "filled" : "text"}
                      disabled={!view.controlsEnabled || busyTarget !== null}
                      onPress={() => void applyChannelMode(output, pick)}
                      testID={`channel-mode-${output.transport}-${output.id}-${pick}`}
                    />
                  ))}
                </Row>
              </Row>
            </Column>
          );
        })}
      </Column>
    </BottomSheet>
  );
}
