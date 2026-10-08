import type {
  ActiveOutput,
  AirPlayMode,
  CdAction,
  CdStatus,
  DiscoveredHost,
  EqGains,
  OutputInfo,
} from "@on-air/api-types";
import {
  activateInput,
  activateOutput,
  controlCd,
  fetchStatus,
  HttpError,
  openBluetoothSettings,
  pairAirplay,
  pairBluetooth,
  setEq,
  setSampleRate,
  setVolume,
  verifyPin,
} from "@on-air/control-client";
import type { ReactNode } from "react";
import { createContext, use, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AppState, type AppStateStatus } from "react-native";
import { formatDesktopTarget, targetBase } from "@/desktop-target";
import { friendlyError, isUnauthorized } from "@/errors";
import { useDesktopConnection } from "@/hooks/useDesktopConnection";
import { useDiscovery } from "@/hooks/useDiscovery";
import { useLatestWriteQueue } from "@/hooks/useLatestWriteQueue";
import { usePairingStore } from "@/hooks/usePairingStore";
import { prettyInput, uniqueByLabel } from "@/pretty-input";
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
  inputs: string[];
  activeInput: string;
  activeInputLabel: string;
  outputs: OutputInfo[];
  activeOutput: ActiveOutput | null;
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

  const connection = useDesktopConnection({ base, token, active: appActive, onUnauthorized });
  const { patch, refreshNow, scheduleRefresh } = connection;

  /** Shows the failure; a 401 from any call ends the pairing. */
  const fail = useCallback(
    (error: unknown, action: string) => {
      const message = friendlyError(error, action);
      if (isUnauthorized(error)) onUnauthorized(message);
      else setActionError(message);
    },
    [onUnauthorized],
  );

  const volumeQueue = useLatestWriteQueue<number>(
    async (value) => {
      const creds = credentials.current;
      if (creds) await setVolume(creds.base, value, creds.token);
    },
    (error) => fail(error, "change the volume"),
  );
  const eqQueue = useLatestWriteQueue<EqGains>(
    async (value) => {
      const creds = credentials.current;
      if (creds) await setEq(creds.base, value, creds.token);
    },
    (error) => fail(error, "change the equalizer"),
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

  const gainsRef = useRef(connection.gains);
  gainsRef.current = connection.gains;

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
        return false;
      } finally {
        releaseBusy();
      }
    },
    [activateNow, claim, fail, releaseBusy],
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
      patch({ volume: value });
      pushVolume(value);
    },
    [patch, pushVolume],
  );

  const { push: pushEq } = eqQueue;
  const applyEq = useCallback(
    (gains: EqGains) => {
      gainsRef.current = gains;
      patch({ gains });
      pushEq(gains);
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
    casting: Boolean(connection.activeOutput),
  });
  const view = useMemo(() => describeUiState(uiState), [uiState]);
  const inputs = useMemo(() => uniqueByLabel(connection.inputs), [connection.inputs]);
  const error = actionError ?? (saved ? connection.error : null);
  const pairedHost = saved ? formatDesktopTarget(saved) : "";

  const value = useMemo<RemoteSession>(
    () => ({
      uiState,
      view,
      error,
      warning: store.warning,
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
      controlCd: cdAction,
    }),
    [
      uiState,
      view,
      error,
      store.warning,
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
