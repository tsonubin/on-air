import { BottomSheet, Button, Column, Row, Spacer, Text } from "@expo/ui";
import type { OutputInfo, OutputPhase, OutputSyncStatus } from "@on-air/api-types";
import { outputPhase, TRANSPORT_LABELS } from "@on-air/api-types";
import {
  SYNC_STATE_LABELS,
  SYNC_TRIM_DESCRIPTION,
  SYNC_TRIM_LIMIT_MS,
} from "@on-air/control-client";
import { useEffect, useState } from "react";
import { View } from "react-native";
import { useRemoteSession } from "@/remote-session";
import { colors, theme } from "@/theme";
import { sheetProps } from "./sheet-props";

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

/** One output's sync line: state, measured drift and any manual trim. */
export function syncSummary(row: OutputSyncStatus): string {
  const drift = row.drift_ppm === undefined ? "" : ` · ${Math.round(row.drift_ppm)} ppm`;
  const trim = row.trim_ms === 0 ? "" : ` · trim ${row.trim_ms > 0 ? "+" : ""}${row.trim_ms} ms`;
  return `${SYNC_STATE_LABELS[row.state]}${drift}${trim}`;
}

/**
 * The manual latency trim of one output, stepped by ear. The copy insists
 * on the truth: a trim is a correction, never a calibrated measurement.
 */
function TrimEditor({
  output,
  status,
  disabled,
  onApply,
}: {
  output: OutputInfo;
  status: OutputSyncStatus;
  disabled: boolean;
  onApply(trimMs: number): void;
}) {
  const [draft, setDraft] = useState(status.trim_ms);
  useEffect(() => {
    setDraft(status.trim_ms);
  }, [status.trim_ms]);
  const clamp = (value: number) =>
    Math.max(-SYNC_TRIM_LIMIT_MS, Math.min(SYNC_TRIM_LIMIT_MS, value));
  return (
    <View
      style={{
        borderWidth: 1,
        borderColor: colors.border,
        borderRadius: theme.radius.md,
        padding: theme.spacing.md,
        gap: theme.spacing.sm,
      }}
      testID="trim-editor"
    >
      <Text textStyle={{ fontWeight: "600" }}>{`Latency trim · ${output.name}`}</Text>
      <Text textStyle={{ color: colors.secondaryLabel }}>{SYNC_TRIM_DESCRIPTION}</Text>
      <Row alignment="center" spacing={theme.spacing.sm}>
        <Button
          label="−10"
          variant="outlined"
          disabled={disabled}
          onPress={() => setDraft((value) => clamp(value - 10))}
          testID="trim-minus-10"
        />
        <Button
          label="−1"
          variant="outlined"
          disabled={disabled}
          onPress={() => setDraft((value) => clamp(value - 1))}
          testID="trim-minus-1"
        />
        <Text textStyle={{ fontWeight: "600" }}>{`${draft > 0 ? "+" : ""}${draft} ms`}</Text>
        <Button
          label="+1"
          variant="outlined"
          disabled={disabled}
          onPress={() => setDraft((value) => clamp(value + 1))}
          testID="trim-plus-1"
        />
        <Button
          label="+10"
          variant="outlined"
          disabled={disabled}
          onPress={() => setDraft((value) => clamp(value + 10))}
          testID="trim-plus-10"
        />
        <Spacer />
        <Button
          label="Set"
          variant="text"
          disabled={disabled || draft === status.trim_ms}
          onPress={() => onApply(draft)}
          testID="set-trim"
        />
      </Row>
    </View>
  );
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
    sync,
    applyTrim,
  } = useRemoteSession();

  const choose = async (output: OutputInfo) => {
    if (output.needs_pair && !output.paired) {
      onPairRequest(output);
      return;
    }
    if (await activate(output)) onClose();
  };

  const activeSyncRow = activeOutput
    ? sync.find(
        (row) =>
          row.transport === activeOutput.transport && row.device_id === activeOutput.device_id,
      )
    : undefined;
  const activeSyncOutput = activeSyncRow
    ? outputs.find(
        (o) => o.transport === activeSyncRow.transport && o.id === activeSyncRow.device_id,
      )
    : undefined;

  return (
    <BottomSheet
      {...sheetProps({ kind: "output", count: outputs.length, trim: Boolean(activeSyncRow) })}
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
          const phase = selected && activeOutput ? outputPhase(activeOutput) : null;
          const desktopOnly = output.transport === "airplay" && airplayMode === "avroute-picker";
          const working = busyTarget === `${output.transport}:${output.id}`;
          const stereoPair = output.member_count >= 2 || output.kind === "pair";
          const syncRow = sync.find(
            (row) => row.transport === output.transport && row.device_id === output.id,
          );
          return (
            <Button
              key={`${output.transport}-${output.id}`}
              variant={selected && phase === "live" ? "filled" : "outlined"}
              disabled={!view.controlsEnabled || busyTarget !== null || desktopOnly}
              onPress={() => void choose(output)}
              testID={`output-${output.transport}-${output.id}`}
            >
              <Row alignment="center" spacing={theme.spacing.sm}>
                <Column spacing={theme.spacing.xs}>
                  <Text textStyle={{ fontWeight: "600" }}>{output.name}</Text>
                  <Text>{`${stereoPair ? "Stereo pair · " : ""}${TRANSPORT_LABELS[output.transport]}`}</Text>
                  {syncRow && (
                    <Text textStyle={{ color: colors.tertiaryLabel, fontSize: 13 }}>
                      {syncSummary(syncRow)}
                    </Text>
                  )}
                </Column>
                <Spacer />
                <Text>{outputAction({ desktopOnly, working, selected, phase, output })}</Text>
              </Row>
            </Button>
          );
        })}
        {activeSyncRow && activeSyncOutput && (
          <TrimEditor
            output={activeSyncOutput}
            status={activeSyncRow}
            disabled={!view.controlsEnabled}
            onApply={(trimMs) => void applyTrim(activeSyncOutput, trimMs)}
          />
        )}
      </Column>
    </BottomSheet>
  );
}
