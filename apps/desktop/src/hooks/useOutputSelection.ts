import type { AirPlayMode, OutputInfo, Transport } from "@on-air/api-types";
import { useCallback, useRef, useState } from "react";
import { type ClientLike, liveClient } from "../lib/client";
import { invokeCommand } from "../lib/tauri";
import type { CoreSnapshotHandle } from "./useCoreSnapshot";

export interface PairTarget {
  transport: Transport;
  id: string;
  name: string;
}

export type DeviceHelpTopic = "bluetooth" | "airplay";

export interface UseOutputSelectionOptions {
  core: Pick<CoreSnapshotHandle, "snapshot" | "refresh" | "reportError" | "clearError" | "patch">;
  client?: ClientLike;
}

export interface OutputSelectionHandle {
  pairTarget: PairTarget | null;
  pairing: boolean;
  /** `"<transport>-<id>"` of the output whose activation is in flight. */
  connectingOutput: string | null;
  deviceHelp: DeviceHelpTopic | null;
  refreshingDevices: boolean;
  chooseInput(name: string): Promise<void>;
  chooseOutput(output: OutputInfo): Promise<void>;
  submitPair(pin: string): Promise<void>;
  cancelPair(): void;
  openBluetoothSettings(): void;
  showAirplayHelp(): void;
  openAirplayPicker(): Promise<void>;
  dismissHelp(): void;
  refreshDevices(): Promise<void>;
}

export function outputKey(output: Pick<OutputInfo, "transport" | "id">): string {
  return `${output.transport}-${output.id}`;
}

/** AirPlay via the macOS route picker cannot be driven from this window. */
export function isPickerOnly(output: OutputInfo, airplayMode: AirPlayMode | null): boolean {
  return output.transport === "airplay" && airplayMode === "avroute-picker";
}

/**
 * Choose → (needs pairing? PairDialog) → activate. One activation at a time;
 * while one is in flight every other row is disabled.
 */
export function useOutputSelection({
  core,
  client = liveClient,
}: UseOutputSelectionOptions): OutputSelectionHandle {
  const { snapshot, refresh, reportError, clearError, patch } = core;
  const [pairTarget, setPairTarget] = useState<PairTarget | null>(null);
  const [pairing, setPairing] = useState(false);
  const [connectingOutput, setConnectingOutput] = useState<string | null>(null);
  const [deviceHelp, setDeviceHelp] = useState<DeviceHelpTopic | null>(null);
  const [refreshingDevices, setRefreshingDevices] = useState(false);
  const connecting = useRef(false);

  const perform = useCallback(
    async (action: () => Promise<void>): Promise<boolean> => {
      clearError();
      try {
        await action();
        return true;
      } catch (err) {
        reportError(err);
        return false;
      }
    },
    [clearError, reportError],
  );

  const chooseInput = useCallback(
    async (name: string) => {
      await perform(async () => {
        await client.activateInput(name);
        patch.activeInput(name);
        await refresh("devices");
      });
    },
    [client, patch, perform, refresh],
  );

  const activate = useCallback(
    async (output: OutputInfo): Promise<boolean> => {
      if (connecting.current) return false;
      connecting.current = true;
      setConnectingOutput(outputKey(output));
      try {
        return await perform(async () => {
          await client.activateOutput(output.transport, output.id);
          patch.activeOutput({
            transport: output.transport,
            device_id: output.id,
            device_name: output.name,
          });
          await refresh("devices");
        });
      } finally {
        connecting.current = false;
        setConnectingOutput(null);
      }
    },
    [client, patch, perform, refresh],
  );

  const chooseOutput = useCallback(
    async (output: OutputInfo) => {
      if (isPickerOnly(output, snapshot.airplayMode)) {
        setDeviceHelp("airplay");
        return;
      }
      if (output.needs_pair && !output.paired) {
        setPairTarget({ transport: output.transport, id: output.id, name: output.name });
        return;
      }
      await activate(output);
    },
    [activate, snapshot.airplayMode],
  );

  const submitPair = useCallback(
    async (pin: string) => {
      if (!pairTarget || pairing) return;
      setPairing(true);
      const paired = await perform(async () => {
        if (pairTarget.transport === "airplay") await client.pairAirplay(pairTarget.id, pin);
        else if (pairTarget.transport === "bluetooth") await client.pairBluetooth(pairTarget.id);
      });
      setPairing(false);
      if (!paired) return;
      const output = snapshot.outputs?.find(
        (o) => o.transport === pairTarget.transport && o.id === pairTarget.id,
      );
      if (!output || (await activate(output))) setPairTarget(null);
    },
    [activate, client, pairTarget, pairing, perform, snapshot.outputs],
  );

  const cancelPair = useCallback(() => {
    if (!pairing) setPairTarget(null);
  }, [pairing]);

  const refreshDevices = useCallback(async () => {
    if (refreshingDevices) return;
    setRefreshingDevices(true);
    try {
      await refresh("devices");
    } finally {
      setRefreshingDevices(false);
    }
  }, [refresh, refreshingDevices]);

  const openBluetoothSettings = useCallback(() => {
    setDeviceHelp("bluetooth");
    void perform(() => client.openBluetoothSettings());
  }, [client, perform]);

  const openAirplayPicker = useCallback(async () => {
    await perform(async () => {
      await invokeCommand<string>("open_airplay_picker");
    });
  }, [perform]);

  return {
    pairTarget,
    pairing,
    connectingOutput,
    deviceHelp,
    refreshingDevices,
    chooseInput,
    chooseOutput,
    submitPair,
    cancelPair,
    openBluetoothSettings,
    showAirplayHelp: () => setDeviceHelp("airplay"),
    openAirplayPicker,
    dismissHelp: () => setDeviceHelp(null),
    refreshDevices,
  };
}
