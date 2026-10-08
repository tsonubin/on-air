import { BottomSheet, Button, Column, Row, Spacer, Text } from "@expo/ui";
import type { OutputInfo } from "@on-air/api-types";
import { useRemoteSession } from "@/remote-session";
import { theme } from "@/theme";
import { sheetProps } from "./sheet-props";

/** The trailing word on a speaker row. */
export function outputAction({
  desktopOnly,
  working,
  selected,
  output,
}: {
  desktopOnly: boolean;
  working: boolean;
  selected: boolean;
  output: OutputInfo;
}): string {
  if (desktopOnly) return "Desktop only";
  if (working) return "Connecting…";
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
  const { outputs, activeOutput, airplayMode, busyTarget, view, activate, openBluetoothSettings } =
    useRemoteSession();

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
          const desktopOnly = output.transport === "airplay" && airplayMode === "avroute-picker";
          const working = busyTarget === `${output.transport}:${output.id}`;
          const stereoPair = output.member_count >= 2 || output.kind === "pair";
          return (
            <Button
              key={`${output.transport}-${output.id}`}
              variant={selected ? "filled" : "outlined"}
              disabled={!view.controlsEnabled || busyTarget !== null || desktopOnly}
              onPress={() => void choose(output)}
              testID={`output-${output.transport}-${output.id}`}
            >
              <Row alignment="center" spacing={theme.spacing.sm}>
                <Column spacing={theme.spacing.xs}>
                  <Text textStyle={{ fontWeight: "600" }}>{output.name}</Text>
                  <Text>{`${stereoPair ? "Stereo pair · " : ""}${output.transport}`}</Text>
                </Column>
                <Spacer />
                <Text>{outputAction({ desktopOnly, working, selected, output })}</Text>
              </Row>
            </Button>
          );
        })}
      </Column>
    </BottomSheet>
  );
}
