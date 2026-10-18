import type {
  ActiveOutputView,
  AirPlayMode,
  AudioQualityResponse,
  CdAction,
  CdStatus,
  DiscoveredHost,
  DspQuality,
  EqGains,
  FirewallWarning,
  OutputInfo,
  OutputSyncStatus,
  SyncGroupView,
} from "@on-air/api-types";
import { outputPhase } from "@on-air/api-types";
import {
  activateInput,
  activateOutput,
  addSyncGroupMember,
  controlCd,
  createSyncGroup,
  dissolveSyncGroup,
  fetchStatus,
  HttpError,
  type InputLabel,
  inputLabels,
  leaveSyncGroupMember,
  openBluetoothSettings,
  pairAirplay,
  pairBluetooth,
  prettyInput,
  type StereoLevels,
  setAudioQuality,
  setEq,
  setSampleRate,
  setSyncTrim,
  setVolume,
  startSyncGroup,
  stopSyncGroup,
  verifyPin,
} from "@on-air/control-client";
import type { ReactNode } from "react";
import { createContext, use, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AppState, type AppStateStatus } from "react-native";
import { formatDesktopTarget, targetBase } from "@/desktop-target";
import { friendlyError, isUnauthorized } from "@/errors";
import { type AudioPerformanceHandle, useAudioPerformance } from "@/hooks/useAudioPerformance";
import { useDesktopConnection } from "@/hooks/useDesktopConnection";
import { useDiscovery } from "@/hooks/useDiscovery";
import { useLatestWriteQueue } from "@/hooks/useLatestWriteQueue";
import { usePairingStore } from "@/hooks/usePairingStore";
import { deriveUiState, describeUiState, type UiState, type UiView } from "@/ui-state";

export const PIN_LENGTH = 6;
export const DEVICE_PIN_LENGTH = 8;
/** Bluetooth speakers appear a moment after the desktop's settings pane pairs them. */
const BLUETOOTH_FOLLOW_UP_MS = 2_500;

/** Keeps the digits of what was typed or pasted, up to `max`. */
export function digitsOnly(value: string, max: number): string {
  return value.replace(/\D/g, "").slice(0, max);
}

/** iOS reports `inactive` during transitions and `unknown` before the first event. */
export function isForeground(state: AppStateStatus | null | undefined): boolean {
  return state !== "background";
}

function useAppActive(): boolean {
  const [active, setActive] = useState(() => isForeground(AppState.currentState));
  useEffect(() => {
    const subscription = AppState.addEventListener("change", (next) => {
      setActive(isForeground(next));
    });
    return () => subscription.remove();
  }, []);
  return active;
}

export type DevicePairTarget = { transport: OutputInfo["transport"]; id: string; name: string };

export type RemoteState = {
  uiState: UiState;
  view: UiView;
  /** One message for the current screen: the last failed action, else the last refresh. */
  error: string | null;
  /** Non-fatal notice (for example, the pairing could not be saved to the keychain). */
  warning: string | null;
  firewallWarning?: FirewallWarning;
  captureBufferMs?: number;
  audioPerformance: AudioPerformanceHandle;
  /** The latest stereo level reading, or `null` while none is arriving. */
  levels: StereoLevels | null;
  /** DSP quality profiles; `null` on a desktop too old to report them. */
  audioQuality: AudioQualityResponse | null;
  /** Per-output latency trim and sync state; empty on an older desktop. */
  sync: OutputSyncStatus[];
  /** Sync groups, empty when the desktop predates them. */
  syncGroups: SyncGroupView[];
  paired: boolean;
  /** The paired desktop as `host` or `host:port`. */
  pairedHost: string;
  // First-run pairing
  found: DiscoveredHost[];
  scanning: boolean;
  hostInput: string;
  pin: string;
  pairing: boolean;
  // Mixer
  inputs: InputLabel[];
  activeInput: string;
  activeInputLabel: string;
  outputs: OutputInfo[];
  activeOutput: ActiveOutputView | null;
  volume: number;
  gains: EqGains;
  sampleRate: number;
  outputSampleRate: number;
  inputRates: number[];
  outputRates: number[];
  airplayMode: AirPlayMode | null;
  cd: CdStatus;
  /** `input:<name>` or `<transport>:<id>` while that switch is in flight. */
  busyTarget: string | null;
  configuringRate: boolean;
  devicePairing: boolean;
  refreshing: boolean;
};

export type RemoteActions = {
  scan(): void;
  changeHost(text: string): void;
  selectHost(hit: DiscoveredHost): void;
  changePin(text: string): void;
  pair(): Promise<void>;
  disconnect(): void;
  refresh(): Promise<void>;
  /** Resolves `true` once the desktop switched source. */
  pickInput(name: string): Promise<boolean>;
  /** Resolves `true` once the speaker is live. */
  activate(output: OutputInfo): Promise<boolean>;
  /** Pairs (AirPlay PIN or Bluetooth confirm), then activates. */
  pairDevice(target: DevicePairTarget, pin: string): Promise<boolean>;
  openBluetoothSettings(): Promise<void>;
  applyRate(kind: "input" | "output", hz: number): Promise<void>;
  applyVolume(value: number): void;
  applyEq(gains: EqGains): void;
  changeBand(index: number, value: number): void;
  /** Overrides the DSP quality profile. */
  applyQuality(quality: DspQuality): Promise<void>;
  /** One output's manual latency trim in milliseconds. */
  applyTrim(output: OutputInfo, trimMs: number): Promise<void>;
  /**
   * Forms an empty named group of speakers that play in sync — never one
   * merged device. Resolves `true` once the group exists.
   */
  createGroup(name: string): Promise<boolean>;
  /**
   * Puts one speaker into a group. Membership is configuration: it takes
   * effect at the group's next start together. Resolves `true` on success.
   */
  joinGroup(output: OutputInfo, groupId: number): Promise<boolean>;
  /**
   * Takes one speaker out of its group, stopping it cleanly if it plays
   * along. Resolves `true` on success.
   */
  leaveGroup(output: OutputInfo): Promise<boolean>;
  /** Starts every member at one presentation epoch: the speakers begin together. */
  startGroup(groupId: number): Promise<void>;
  /** Stops every member cleanly; the group stays configured. */
  stopGroup(groupId: number): Promise<void>;
  /** Stops the members and forgets the group. */
  dissolveGroup(groupId: number): Promise<void>;
  controlCd(action: Extract<CdAction, "play" | "pause" | "next" | "prev">): Promise<void>;
};

export type RemoteSession = RemoteState & RemoteActions;

const RemoteContext = createContext<RemoteSession | null>(null);

export function RemoteProvider({ children }: { children: ReactNode }) {
  const appActive = useAppActive();
  const store = usePairingStore({ appActive });
  const discovery = useDiscovery({ enabled: store.hydrated && !store.pairing });
  const [pin, setPin] = useState("");
  const [pairing, setPairing] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [busyTarget, setBusyTarget] = useState<string | null>(null);
  const [configuringRate, setConfiguringRate] = useState(false);
  const [devicePairing, setDevicePairing] = useState(false);

  const saved = store.pairing;
  const base = saved ? targetBase(saved) : null;
  const token = saved?.token ?? null;
  const credentials = useRef<{ base: string; token: string } | null>(null);
  credentials.current = base && token ? { base, token } : null;

  const { selectHost: discoverySelect } = discovery;
  const { clear: clearPairing } = store;
  const onUnauthorized = useCallback(
    (message: string) => {
      const lost = store.pairing;
      clearPairing();
      // Keep the desktop selected so re-pairing only needs the new code.
      if (lost) discoverySelect({ host: lost.host, port: lost.port });
      setActionError(message);
    },
    [clearPairing, discoverySelect, store.pairing],
  );

  const connection = useDesktopConnection({
    base,
    token,
    active: appActive,
    onUnauthorized,
    onNotice: setActionError,
  });
  const { patch, refreshNow, scheduleRefresh } = connection;
  const audioPerformance = useAudioPerformance(base, token, () => refreshNow(["status"]));

  /** Shows the failure; a 401 from any call ends the pairing. */
  const fail = useCallback(
    (error: unknown, action: string) => {
      const message = friendlyError(error, action);
      if (isUnauthorized(error)) onUnauthorized(message);
      else setActionError(message);
    },
    [onUnauthorized],
  );

  const gainsRef = useRef(connection.gains);
  gainsRef.current = connection.gains;
  const volumeRef = useRef(connection.volume);
  volumeRef.current = connection.volume;

  // On a final failure (nothing newer queued) the optimistic patch goes back
  // to the value the desktop holds; without one, read it back instead.
  const volumeQueue = useLatestWriteQueue<number>(
    async (value) => {
      const creds = credentials.current;
      if (creds) await setVolume(creds.base, value, creds.token);
    },
    (error, _value, committed) => {
      if (committed === undefined) scheduleRefresh("control", ["volume"]);
      else patch({ volume: committed });
      fail(error, "change the volume");
    },
  );
  const eqQueue = useLatestWriteQueue<EqGains>(
    async (value) => {
      const creds = credentials.current;
      if (creds) await setEq(creds.base, value, creds.token);
    },
    (error, _value, committed) => {
      if (committed === undefined) {
        scheduleRefresh("control", ["gains"]);
      } else {
        gainsRef.current = committed;
        patch({ gains: committed });
      }
      fail(error, "change the equalizer");
    },
  );
  const { reset: resetVolume } = volumeQueue;
  const { reset: resetEq } = eqQueue;

  // A different pairing (or none) must never receive writes queued for the old one.
  const followUp = useRef<ReturnType<typeof setTimeout> | null>(null);
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs on purpose whenever the pairing (base, token) changes.
  useEffect(() => {
    resetVolume();
    resetEq();
    setBusyTarget(null);
    return () => {
      if (followUp.current) clearTimeout(followUp.current);
      followUp.current = null;
    };
  }, [base, token, resetVolume, resetEq]);

  const { scan: discoveryScan, changeHost: discoveryChangeHost, reset: resetDiscovery } = discovery;
  const scan = useCallback(() => void discoveryScan(), [discoveryScan]);
  const changePin = useCallback((text: string) => setPin(digitsOnly(text, PIN_LENGTH)), []);

  const pairingRef = useRef(false);
  const target = discovery.target;
  const pinRef = useRef(pin);
  pinRef.current = pin;
  const { save } = store;
  const pair = useCallback(async () => {
    const code = digitsOnly(pinRef.current, PIN_LENGTH);
    if (pairingRef.current) return;
    if (code.length !== PIN_LENGTH || !target) {
      setActionError("Enter the desktop LAN address and its six-digit pairing code.");
      return;
    }
    pairingRef.current = true;
    setPairing(true);
    setActionError(null);
    try {
      const pairingBase = targetBase(target);
      const status = await fetchStatus(pairingBase);
      if (status.service_enabled === false) {
        throw new HttpError("/api/status", 503, "", "service_paused");
      }
      const nextToken = await verifyPin(pairingBase, code);
      // In memory first: a keychain failure must not throw away a consumed PIN.
      save({ host: target.host, port: target.port, token: nextToken });
      setPin("");
    } catch (error) {
      setActionError(friendlyError(error, "pair with the desktop"));
    } finally {
      pairingRef.current = false;
      setPairing(false);
    }
  }, [save, target]);

  const disconnect = useCallback(() => {
    clearPairing();
    resetDiscovery();
    setPin("");
    setActionError(null);
  }, [clearPairing, resetDiscovery]);

  const refresh = useCallback(async () => {
    setActionError(null);
    await refreshNow();
  }, [refreshNow]);

  const busyRef = useRef<string | null>(null);
  /** One source or speaker switch at a time. */
  const claim = useCallback((key: string) => {
    if (busyRef.current) return false;
    busyRef.current = key;
    setBusyTarget(key);
    return true;
  }, []);
  const releaseBusy = useCallback(() => {
    busyRef.current = null;
    setBusyTarget(null);
  }, []);

  const pickInput = useCallback(
    async (name: string) => {
      const creds = credentials.current;
      if (!creds || !name || !claim(`input:${name}`)) return false;
      setActionError(null);
      try {
        await activateInput(creds.base, name, creds.token);
        await refreshNow(["activeInput", "sampleRate", "cd"]);
        return true;
      } catch (error) {
        fail(error, "change the source");
        return false;
      } finally {
        releaseBusy();
      }
    },
    [claim, fail, refreshNow, releaseBusy],
  );

  const activateNow = useCallback(
    async (output: OutputInfo) => {
      const creds = credentials.current;
      if (!creds) return;
      await activateOutput(creds.base, output.transport, output.id, creds.token);
      await refreshNow(["activeOutput", "volume", "sampleRate"]);
    },
    [refreshNow],
  );

  const activate = useCallback(
    async (output: OutputInfo) => {
      if (!credentials.current || !claim(`${output.transport}:${output.id}`)) return false;
      setActionError(null);
      try {
        await activateNow(output);
        return true;
      } catch (error) {
        fail(error, "connect that speaker");
        // A failed start may leave the desktop idle, rolled back or `failed`.
        scheduleRefresh("control", ["activeOutput"]);
        return false;
      } finally {
        releaseBusy();
      }
    },
    [activateNow, claim, fail, releaseBusy, scheduleRefresh],
  );

  const outputsRef = useRef(connection.outputs);
  outputsRef.current = connection.outputs;
  const devicePairingRef = useRef(false);
  const pairDevice = useCallback(
    async (device: DevicePairTarget, devicePin: string) => {
      const creds = credentials.current;
      if (!creds || devicePairingRef.current) return false;
      devicePairingRef.current = true;
      setDevicePairing(true);
      setActionError(null);
      try {
        if (device.transport === "airplay") {
          await pairAirplay(
            creds.base,
            device.id,
            digitsOnly(devicePin, DEVICE_PIN_LENGTH),
            creds.token,
          );
        } else if (device.transport === "bluetooth") {
          await pairBluetooth(creds.base, device.id, creds.token);
        }
        const output = outputsRef.current.find(
          (candidate) => candidate.transport === device.transport && candidate.id === device.id,
        );
        if (output) await activateNow(output);
        scheduleRefresh("control", ["outputs"]);
        return true;
      } catch (error) {
        fail(error, "pair that speaker");
        return false;
      } finally {
        devicePairingRef.current = false;
        setDevicePairing(false);
      }
    },
    [activateNow, fail, scheduleRefresh],
  );

  const openBluetooth = useCallback(async () => {
    const creds = credentials.current;
    if (!creds) return;
    setActionError(null);
    try {
      await openBluetoothSettings(creds.base, creds.token);
      await refreshNow(["outputs"]);
      if (followUp.current) clearTimeout(followUp.current);
      followUp.current = setTimeout(() => {
        followUp.current = null;
        scheduleRefresh("control", ["outputs"]);
      }, BLUETOOTH_FOLLOW_UP_MS);
    } catch (error) {
      fail(error, "open Bluetooth settings");
    }
  }, [fail, refreshNow, scheduleRefresh]);

  const ratesRef = useRef({ input: connection.sampleRate, output: connection.outputSampleRate });
  ratesRef.current = { input: connection.sampleRate, output: connection.outputSampleRate };
  const configuringRef = useRef(false);
  const applyRate = useCallback(
    async (kind: "input" | "output", hz: number) => {
      const creds = credentials.current;
      if (!creds || configuringRef.current) return;
      const previous = ratesRef.current[kind];
      const key = kind === "input" ? "sampleRate" : "outputSampleRate";
      configuringRef.current = true;
      setConfiguringRate(true);
      setActionError(null);
      patch({ [key]: hz });
      try {
        await setSampleRate(
          creds.base,
          kind === "input" ? { input_hz: hz } : { output_hz: hz },
          creds.token,
        );
        await refreshNow(["sampleRate"]);
      } catch (error) {
        patch({ [key]: previous });
        fail(error, `change the ${kind} sample rate`);
      } finally {
        configuringRef.current = false;
        setConfiguringRate(false);
      }
    },
    [fail, patch, refreshNow],
  );

  const { push: pushVolume } = volumeQueue;
  const applyVolume = useCallback(
    (value: number) => {
      const shown = volumeRef.current;
      volumeRef.current = value;
      patch({ volume: value });
      pushVolume(value, shown);
    },
    [patch, pushVolume],
  );

  const { push: pushEq } = eqQueue;
  const applyEq = useCallback(
    (gains: EqGains) => {
      const shown = gainsRef.current;
      gainsRef.current = gains;
      patch({ gains });
      pushEq(gains, shown);
    },
    [patch, pushEq],
  );

  const changeBand = useCallback(
    (index: number, value: number) => {
      const next = [...gainsRef.current] as EqGains;
      next[index] = value;
      applyEq(next);
    },
    [applyEq],
  );

  const applyQuality = useCallback(
    async (quality: DspQuality) => {
      const creds = credentials.current;
      if (!creds) return;
      setActionError(null);
      try {
        await setAudioQuality(creds.base, quality, creds.token);
        await refreshNow(["audioQuality"]);
      } catch (error) {
        fail(error, "change the DSP quality");
      }
    },
    [fail, refreshNow],
  );

  const applyTrim = useCallback(
    async (output: OutputInfo, trimMs: number) => {
      const creds = credentials.current;
      if (!creds) return;
      setActionError(null);
      try {
        await setSyncTrim(
          creds.base,
          { transport: output.transport, device_id: output.id, trim_ms: trimMs },
          creds.token,
        );
        await refreshNow(["sync"]);
      } catch (error) {
        fail(error, "change the latency trim");
      }
    },
    [fail, refreshNow],
  );

  const syncRowsRef = useRef(connection.sync);
  syncRowsRef.current = connection.sync;
  const createGroup = useCallback(
    async (name: string) => {
      const creds = credentials.current;
      if (!creds) return false;
      setActionError(null);
      try {
        await createSyncGroup(creds.base, name, creds.token);
        await refreshNow(["sync"]);
        return true;
      } catch (error) {
        fail(error, "create the group");
        return false;
      }
    },
    [fail, refreshNow],
  );

  const joinGroup = useCallback(
    async (output: OutputInfo, groupId: number) => {
      const creds = credentials.current;
      if (!creds) return false;
      setActionError(null);
      try {
        await addSyncGroupMember(
          creds.base,
          groupId,
          { transport: output.transport, device_id: output.id },
          creds.token,
        );
        await refreshNow(["sync"]);
        return true;
      } catch (error) {
        fail(error, "add that speaker to the group");
        return false;
      }
    },
    [fail, refreshNow],
  );

  const leaveGroup = useCallback(
    async (output: OutputInfo) => {
      const creds = credentials.current;
      // The row's `group_id` is the membership of record; without one there
      // is no group to leave.
      const groupId = syncRowsRef.current.find(
        (row) => row.transport === output.transport && row.device_id === output.id,
      )?.group_id;
      if (!creds || groupId === undefined) return false;
      setActionError(null);
      try {
        await leaveSyncGroupMember(
          creds.base,
          groupId,
          { transport: output.transport, device_id: output.id },
          creds.token,
        );
        await refreshNow(["sync"]);
        return true;
      } catch (error) {
        fail(error, "take that speaker out of the group");
        return false;
      }
    },
    [fail, refreshNow],
  );

  const startGroup = useCallback(
    async (groupId: number) => {
      const creds = credentials.current;
      if (!creds) return;
      setActionError(null);
      try {
        await startSyncGroup(creds.base, groupId, creds.token);
        await refreshNow(["sync"]);
      } catch (error) {
        fail(error, "start the speakers together");
      }
    },
    [fail, refreshNow],
  );

  const stopGroup = useCallback(
    async (groupId: number) => {
      const creds = credentials.current;
      if (!creds) return;
      setActionError(null);
      try {
        await stopSyncGroup(creds.base, groupId, creds.token);
        await refreshNow(["sync"]);
      } catch (error) {
        fail(error, "stop the speakers");
      }
    },
    [fail, refreshNow],
  );

  const dissolveGroup = useCallback(
    async (groupId: number) => {
      const creds = credentials.current;
      if (!creds) return;
      setActionError(null);
      try {
        await dissolveSyncGroup(creds.base, groupId, creds.token);
        await refreshNow(["sync"]);
      } catch (error) {
        fail(error, "dissolve the group");
      }
    },
    [fail, refreshNow],
  );

  const cdAction = useCallback(
    async (action: Extract<CdAction, "play" | "pause" | "next" | "prev">) => {
      const creds = credentials.current;
      if (!creds) return;
      setActionError(null);
      try {
        patch({ cd: await controlCd(creds.base, action, creds.token) });
      } catch (error) {
        fail(error, "control the compact disc");
      }
    },
    [fail, patch],
  );

  const uiState = deriveUiState({
    hydrated: store.hydrated,
    paired: Boolean(saved),
    phase: connection.phase,
    output: connection.activeOutput ? outputPhase(connection.activeOutput) : null,
  });
  const view = useMemo(() => describeUiState(uiState), [uiState]);
  const inputs = useMemo(() => inputLabels(connection.inputs), [connection.inputs]);
  const error = actionError ?? (saved ? connection.error : null);
  const pairedHost = saved ? formatDesktopTarget(saved) : "";

  const value = useMemo<RemoteSession>(
    () => ({
      uiState,
      view,
      error,
      warning: store.warning,
      firewallWarning: connection.status?.firewall_warning,
      captureBufferMs: connection.status?.capture_buffer_ms,
      audioPerformance,
      levels: connection.levels,
      audioQuality: connection.audioQuality,
      sync: connection.sync,
      syncGroups: connection.syncGroups,
      paired: Boolean(saved),
      pairedHost,
      found: discovery.found,
      scanning: discovery.scanning,
      hostInput: discovery.host,
      pin,
      pairing,
      inputs,
      activeInput: connection.activeInput,
      activeInputLabel: connection.activeInput ? prettyInput(connection.activeInput) : "",
      outputs: connection.outputs,
      activeOutput: connection.activeOutput,
      volume: connection.volume,
      gains: connection.gains,
      sampleRate: connection.sampleRate,
      outputSampleRate: connection.outputSampleRate,
      inputRates: connection.inputRates,
      outputRates: connection.outputRates,
      airplayMode: connection.airplayMode,
      cd: connection.cd,
      busyTarget,
      configuringRate,
      devicePairing,
      refreshing: connection.refreshing,
      scan,
      changeHost: discoveryChangeHost,
      selectHost: discoverySelect,
      changePin,
      pair,
      disconnect,
      refresh,
      pickInput,
      activate,
      pairDevice,
      openBluetoothSettings: openBluetooth,
      applyRate,
      applyVolume,
      applyEq,
      changeBand,
      applyQuality,
      applyTrim,
      createGroup,
      joinGroup,
      leaveGroup,
      startGroup,
      stopGroup,
      dissolveGroup,
      controlCd: cdAction,
    }),
    [
      uiState,
      view,
      error,
      store.warning,
      connection.status?.firewall_warning,
      connection.status?.capture_buffer_ms,
      audioPerformance,
      saved,
      pairedHost,
      discovery.found,
      discovery.scanning,
      discovery.host,
      pin,
      pairing,
      inputs,
      connection.activeInput,
      connection.outputs,
      connection.activeOutput,
      connection.volume,
      connection.gains,
      connection.sampleRate,
      connection.outputSampleRate,
      connection.inputRates,
      connection.outputRates,
      connection.airplayMode,
      connection.cd,
      connection.levels,
      connection.audioQuality,
      connection.sync,
      connection.syncGroups,
      busyTarget,
      configuringRate,
      devicePairing,
      connection.refreshing,
      scan,
      discoveryChangeHost,
      discoverySelect,
      changePin,
      pair,
      disconnect,
      refresh,
      pickInput,
      activate,
      pairDevice,
      openBluetooth,
      applyRate,
      applyVolume,
      applyEq,
      changeBand,
      applyQuality,
      applyTrim,
      createGroup,
      joinGroup,
      leaveGroup,
      startGroup,
      stopGroup,
      dissolveGroup,
      cdAction,
    ],
  );

  return <RemoteContext value={value}>{children}</RemoteContext>;
}

export function useRemoteSession(): RemoteSession {
  const session = use(RemoteContext);
  if (!session) throw new Error("RemoteProvider is required");
  return session;
}
