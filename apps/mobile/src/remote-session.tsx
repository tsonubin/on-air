import {
  BottomSheet,
  Button,
  Column,
  FieldGroup,
  Host,
  Picker,
  Row,
  Spacer,
  Text,
  TextInput,
  useNativeState,
} from "@expo/ui";
import { environment, presentationBackground, tint } from "@expo/ui/swift-ui/modifiers";
import type {
  ActiveOutput,
  CdStatus,
  DiscoveredHost,
  OutputInfo,
  StatusResponse,
  WsEvent,
} from "@on-air/api-types";
import { DEFAULT_PORT } from "@on-air/api-types";
import { router } from "expo-router";
import { StatusBar } from "expo-status-bar";
import type React from "react";
import { createContext, use, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AppState, Platform, View } from "react-native";
import { subscribeToDesktop } from "@/connection-events";
import {
  activateInput,
  activateOutput,
  apiBase,
  controlCd,
  discoverOnAir,
  fetchStatus,
  getActiveInput,
  getActiveOutput,
  getAirplayMode,
  getCd,
  getEq,
  getSampleRate,
  getVolume,
  HttpError,
  listInputs,
  listOutputs,
  openBluetoothSettings,
  pairAirplay,
  pairBluetooth,
  prettyInput,
  setEq,
  setSampleRate,
  setVolume,
  verifyPin,
  wsUrl,
} from "@/controlClient";
import { MixerHome, mobileColors } from "@/mixer-home";
import { PairingHome } from "@/pairing-home";
import { clearPairing, loadPairing, savePairing } from "@/pairing-store";
import { theme } from "@/theme";

// BottomSheet creates its own native Host, so the app Host's theme does not cross it.
const sheetPresentation =
  Platform.OS === "ios"
    ? [
        environment({ key: "colorScheme", value: "dark" }),
        presentationBackground(mobileColors.surface),
        tint(mobileColors.accent),
      ]
    : undefined;
type PairTarget = { transport: string; id: string; name: string };
type QueuedControl<T> = {
  base: string;
  token: string;
  connection: number;
  value: T;
};

function normalizeDesktopHost(value: string): string {
  return value
    .trim()
    .replace(/^https?:\/\//i, "")
    .replace(/\/.*$/, "")
    .replace(/:\d+$/, "");
}

function friendlyError(error: unknown, action = "complete that action"): string {
  if (error instanceof HttpError) {
    if (error.status === 401) return "Pairing expired or the code was not accepted. Pair again.";
    if (error.status === 429) return "Too many pairing attempts. Wait a moment and try again.";
    if (error.status === 503)
      return "The desktop service is paused. Turn it on from the tray menu.";
    if (error.status === 404) return "That device is no longer available. Refresh and try again.";
  }
  return `Could not ${action}. Check that both devices are on the same Wi-Fi and try again.`;
}

async function localIpv4(): Promise<string | undefined> {
  try {
    const Network = await import("expo-network");
    return await Network.getIpAddressAsync();
  } catch {
    return undefined;
  }
}

function useRemoteController() {
  const [refreshing, setRefreshing] = useState(false);
  const [connectionState, setConnectionState] = useState<
    "connecting" | "connected" | "reconnecting"
  >("connecting");
  const [hydrated, setHydrated] = useState(false);
  const [host, setHost] = useState("127.0.0.1");
  const [pin, setPin] = useState("");
  const [token, setToken] = useState<string | null>(null);
  const [status, setStatus] = useState<StatusResponse | null>(null);
  const [found, setFound] = useState<DiscoveredHost[]>([]);
  const [scanning, setScanning] = useState(false);
  const [inputs, setInputs] = useState<string[]>([]);
  const [outputs, setOutputs] = useState<OutputInfo[]>([]);
  const [activeInput, setActiveInput] = useState("");
  const [activeOutput, setActiveOutput] = useState<ActiveOutput | null>(null);
  const [volume, setVolumeValue] = useState(50);
  const [gains, setGains] = useState<[number, number, number, number, number]>([0, 0, 0, 0, 0]);
  const [sampleRate, setInRate] = useState(44_100);
  const [outputSampleRate, setOutRate] = useState(44_100);
  const [inputRates, setInputRates] = useState<number[]>([44_100, 48_000]);
  const [outputRates, setOutputRates] = useState<number[]>([44_100, 48_000]);
  const [airplayMode, setAirplayMode] = useState("");
  const [cd, setCd] = useState<CdStatus>({
    present: false,
    playing: false,
    track: 0,
    track_count: 0,
    position_ms: 0,
    duration_ms: 0,
  });
  const [error, setError] = useState<string | null>(null);
  const [pairTarget, setPairTarget] = useState<PairTarget | null>(null);
  const [pairPin, setPairPin] = useState("");
  const [pairing, setPairing] = useState(false);
  const [devicePairing, setDevicePairing] = useState(false);
  const [configuringRate, setConfiguringRate] = useState(false);
  const [busyTarget, setBusyTarget] = useState<string | null>(null);
  const [sourceOpen, setSourceOpen] = useState(false);
  const [outputOpen, setOutputOpen] = useState(false);
  const [appActive, setAppActive] = useState(AppState.currentState === "active");
  const devicePinInput = useNativeState("");
  const didInitialScan = useRef(false);
  const scanInFlight = useRef(false);
  const scanRevision = useRef(0);
  const connectionRevision = useRef(0);
  const refreshInFlight = useRef<Promise<void> | null>(null);
  const refreshQueued = useRef(false);
  const gainsRef = useRef(gains);
  const volumePending = useRef<QueuedControl<number> | null>(null);
  const volumeSending = useRef(false);
  const eqPending = useRef<QueuedControl<typeof gains> | null>(null);
  const eqSending = useRef(false);
  gainsRef.current = gains;

  const invalidateConnection = useCallback(() => {
    connectionRevision.current += 1;
    refreshInFlight.current = null;
    setRefreshing(false);
    setConnectionState("connecting");
    refreshQueued.current = false;
    volumePending.current = null;
    eqPending.current = null;
  }, []);

  const setDesktopHost = useCallback((value: string) => {
    scanRevision.current += 1;
    setHost(value);
  }, []);
  const normalizedHost = normalizeDesktopHost(host);
  const base = apiBase(normalizedHost || "127.0.0.1", DEFAULT_PORT);

  useEffect(() => {
    let mounted = true;
    void loadPairing().then((saved) => {
      if (!mounted) return;
      if (saved) {
        invalidateConnection();
        setDesktopHost(saved.host);
        setToken(saved.token);
      }
      setHydrated(true);
    });
    return () => {
      mounted = false;
    };
  }, [invalidateConnection, setDesktopHost]);

  const scan = useCallback(async () => {
    if (scanInFlight.current) return;
    const revision = scanRevision.current + 1;
    scanRevision.current = revision;
    scanInFlight.current = true;
    setScanning(true);
    setError(null);
    try {
      const localIp = await localIpv4();
      const hits = await discoverOnAir({
        localIp,
        extraHosts: normalizedHost && normalizedHost !== "127.0.0.1" ? [normalizedHost] : [],
      });
      if (scanRevision.current !== revision) return;
      setFound(hits);
      if (hits[0] && (normalizedHost === "127.0.0.1" || !normalizedHost)) {
        setDesktopHost(hits[0].host);
      }
      // The first-run screen owns the empty discovery state. Keep the error slot
      // reserved for a failed pairing attempt so manual setup never opens with a
      // stale red scan warning.
      if (hits.length === 0) setError(null);
    } catch {
      if (scanRevision.current === revision) {
        setFound([]);
        setError(null);
      }
    } finally {
      scanInFlight.current = false;
      setScanning(false);
    }
  }, [normalizedHost, setDesktopHost]);

  const refresh = useCallback((): Promise<void> => {
    if (!token) return Promise.resolve();
    if (refreshInFlight.current) {
      refreshQueued.current = true;
      return refreshInFlight.current;
    }
    setRefreshing(true);
    const run = (async () => {
      const connection = connectionRevision.current;
      do {
        refreshQueued.current = false;
        try {
          const results = await Promise.allSettled([
            fetchStatus(base),
            listInputs(base, token),
            listOutputs(base, token),
            getActiveInput(base, token),
            getActiveOutput(base, token),
            getEq(base, token),
            getSampleRate(base, token),
            getVolume(base, token),
            getAirplayMode(base, token),
            getCd(base, token),
          ]);
          if (connectionRevision.current !== connection) {
            refreshQueued.current = false;
            return;
          }
          // Authentication loss wins over timeouts from any other request.
          const unauthorized = results.find(
            (result) =>
              result.status === "rejected" &&
              result.reason instanceof HttpError &&
              result.reason.status === 401,
          );
          if (unauthorized?.status === "rejected") throw unauthorized.reason;
          const [st, ins, outs, actIn, actOut, eq, sr, savedVolume, ap, disc] = results;
          const paused = st.status === "fulfilled" && st.value.service_enabled === false;
          const reachable =
            paused || results.slice(1, 8).some((result) => result.status === "fulfilled");
          setConnectionState(reachable ? "connected" : "reconnecting");
          if (st.status === "fulfilled") setStatus(st.value);
          if (ins.status === "fulfilled") setInputs(ins.value);
          if (outs.status === "fulfilled") setOutputs(outs.value);
          if (actIn.status === "fulfilled") setActiveInput(actIn.value.name ?? "");
          if (actOut.status === "fulfilled") setActiveOutput(actOut.value);
          if (eq.status === "fulfilled") setGains(eq.value);
          if (sr.status === "fulfilled") {
            setInRate(sr.value.input.sample_rate_hz);
            setOutRate(sr.value.output.sample_rate_hz);
            if (sr.value.input.supported_hz.length) setInputRates(sr.value.input.supported_hz);
            if (sr.value.output.supported_hz.length) setOutputRates(sr.value.output.supported_hz);
          }
          if (savedVolume.status === "fulfilled") setVolumeValue(savedVolume.value);
          if (ap.status === "fulfilled") setAirplayMode(ap.value);
          if (disc.status === "fulfilled") setCd(disc.value);
          const failure = results.slice(0, 8).find((result) => result.status === "rejected");
          if (paused) {
            setError(null);
          } else if (failure) {
            setError(
              reachable
                ? "Desktop connected. Some controls could not refresh. Try again."
                : "Reconnecting to your desktop… Your pairing is saved. Check that both devices are on the same Wi-Fi.",
            );
          } else {
            setError(null);
          }
        } catch (refreshError) {
          if (connectionRevision.current !== connection) return;
          if (refreshError instanceof HttpError && refreshError.status === 401) {
            invalidateConnection();
            setToken(null);
            setStatus(null);
            void clearPairing().catch(() => {});
          } else setConnectionState("reconnecting");
          setError(friendlyError(refreshError, "refresh the mixer"));
          refreshQueued.current = false;
        }
      } while (refreshQueued.current);
    })();
    refreshInFlight.current = run;
    void run.finally(() => {
      if (refreshInFlight.current === run) {
        refreshInFlight.current = null;
        setRefreshing(false);
      }
    });
    return run;
  }, [base, invalidateConnection, token]);

  useEffect(() => {
    if (!hydrated || token || didInitialScan.current) return;
    didInitialScan.current = true;
    void scan();
  }, [hydrated, scan, token]);

  useEffect(() => {
    const subscription = AppState.addEventListener("change", (next) => {
      setAppActive(next === "active");
    });
    return () => subscription.remove();
  }, []);

  useEffect(() => {
    if (!token || !appActive) return;
    void refresh();
    const id = setInterval(() => void refresh(), 15_000);
    let eventRefresh: ReturnType<typeof setTimeout> | undefined;
    const stopEvents = subscribeToDesktop(wsUrl(base, token), {
      onConnected: () => {
        void refresh();
      },
      onInterrupted: () => {
        void refresh();
      },
      onMessage: (data) => {
        try {
          const message = JSON.parse(data) as WsEvent;
          if (message.type === "ServiceStateChanged") {
            setStatus((current) =>
              current ? { ...current, service_enabled: message.enabled } : current,
            );
          } else if (message.type !== "LevelMeter") {
            if (eventRefresh) clearTimeout(eventRefresh);
            eventRefresh = setTimeout(() => void refresh(), 300);
          }
        } catch {
          /* Polling remains the recovery path for malformed events. */
        }
      },
    });
    return () => {
      clearInterval(id);
      if (eventRefresh) clearTimeout(eventRefresh);
      stopEvents();
    };
  }, [token, base, refresh, appActive]);

  const pair = async () => {
    const desktopHost = normalizeDesktopHost(host);
    const normalizedPin = pin.replace(/\D/g, "").slice(0, 6);
    if (pairing || normalizedPin.length !== 6 || !desktopHost) {
      setError("Enter the desktop LAN address and its six-digit pairing code.");
      return;
    }
    scanRevision.current += 1;
    setPairing(true);
    setError(null);
    setDesktopHost(desktopHost);
    try {
      const pairingBase = apiBase(desktopHost, DEFAULT_PORT);
      const nextStatus = await fetchStatus(pairingBase);
      if (nextStatus.service_enabled === false) throw new HttpError("/api/status", 503);
      const nextToken = await verifyPin(pairingBase, normalizedPin);
      await savePairing({ host: desktopHost, token: nextToken });
      invalidateConnection();
      setStatus(nextStatus);
      setToken(nextToken);
    } catch (pairError) {
      setError(friendlyError(pairError, "pair with the desktop"));
    } finally {
      setPairing(false);
    }
  };

  const uniqueInputs = useMemo(() => {
    const seen = new Set<string>();
    return inputs.filter((name) => {
      const label = prettyInput(name);
      if (seen.has(label)) return false;
      seen.add(label);
      return true;
    });
  }, [inputs]);

  const pickInput = async (name: string) => {
    if (!token || busyTarget || !name) return;
    setBusyTarget(`input:${name}`);
    setError(null);
    try {
      await activateInput(base, name, token);
      await refresh();
      setSourceOpen(false);
    } catch (inputError) {
      setError(friendlyError(inputError, "change the source"));
    } finally {
      setBusyTarget(null);
    }
  };

  const activate = async (output: OutputInfo) => {
    if (!token || busyTarget) return;
    setBusyTarget(`${output.transport}:${output.id}`);
    setError(null);
    try {
      await activateOutput(base, output.transport, output.id, token);
      await refresh();
      setOutputOpen(false);
    } finally {
      setBusyTarget(null);
    }
  };

  const chooseOutput = async (output: OutputInfo) => {
    if (output.needs_pair && !output.paired) {
      setOutputOpen(false);
      setPairPin("");
      devicePinInput.value = "";
      setPairTarget({ transport: output.transport, id: output.id, name: output.name });
      return;
    }
    try {
      await activate(output);
    } catch (outputError) {
      setError(friendlyError(outputError, "connect that speaker"));
    }
  };

  const submitDevicePair = async () => {
    if (!pairTarget || !token || devicePairing) return;
    setDevicePairing(true);
    setError(null);
    try {
      if (pairTarget.transport === "airplay") {
        await pairAirplay(base, pairTarget.id, pairPin, token);
      } else if (pairTarget.transport === "bluetooth") {
        await pairBluetooth(base, pairTarget.id, token);
      }
      const output = outputs.find(
        (candidate) =>
          candidate.transport === pairTarget.transport && candidate.id === pairTarget.id,
      );
      setPairTarget(null);
      if (output) await activate(output);
    } catch (deviceError) {
      setError(friendlyError(deviceError, "pair that speaker"));
    } finally {
      setDevicePairing(false);
    }
  };

  const applyRate = async (kind: "input" | "output", hz: number) => {
    if (!token || configuringRate) return;
    const previous = kind === "input" ? sampleRate : outputSampleRate;
    setConfiguringRate(true);
    setError(null);
    if (kind === "input") setInRate(hz);
    else setOutRate(hz);
    try {
      await setSampleRate(base, kind === "input" ? { input_hz: hz } : { output_hz: hz }, token);
      await refresh();
    } catch (rateError) {
      if (kind === "input") setInRate(previous);
      else setOutRate(previous);
      setError(friendlyError(rateError, `change the ${kind} sample rate`));
    } finally {
      setConfiguringRate(false);
    }
  };

  const disconnect = () => {
    scanRevision.current += 1;
    invalidateConnection();
    void clearPairing().catch(() => {});
    setToken(null);
    setStatus(null);
    setInputs([]);
    setOutputs([]);
    setActiveInput("");
    setActiveOutput(null);
    setCd({
      present: false,
      playing: false,
      track: 0,
      track_count: 0,
      position_ms: 0,
      duration_ms: 0,
    });
    setError(null);
    setSourceOpen(false);
    setOutputOpen(false);
    didInitialScan.current = false;
  };

  const drainVolume = async () => {
    if (volumeSending.current) return;
    volumeSending.current = true;
    try {
      while (volumePending.current) {
        const pending = volumePending.current;
        volumePending.current = null;
        if (pending.connection !== connectionRevision.current) continue;
        try {
          await setVolume(pending.base, pending.value, pending.token);
        } catch (volumeError) {
          if (pending.connection === connectionRevision.current && volumePending.current === null) {
            setError(friendlyError(volumeError, "change the volume"));
          }
        }
      }
    } finally {
      volumeSending.current = false;
      if (volumePending.current) void drainVolume();
    }
  };

  const applyCd = async (action: "play" | "pause" | "next" | "prev") => {
    if (!token) return;
    setError(null);
    try {
      const next = await controlCd(base, action, token);
      setCd(next);
      await refresh();
    } catch (cdError) {
      setError(friendlyError(cdError, "control the compact disc"));
    }
  };

  const applyVolume = (value: number) => {
    setVolumeValue(value);
    if (!token) return;
    volumePending.current = {
      base,
      token,
      connection: connectionRevision.current,
      value,
    };
    void drainVolume();
  };

  const drainEq = async () => {
    if (eqSending.current) return;
    eqSending.current = true;
    try {
      while (eqPending.current) {
        const pending = eqPending.current;
        eqPending.current = null;
        if (pending.connection !== connectionRevision.current) continue;
        try {
          await setEq(pending.base, pending.value, pending.token);
        } catch (eqError) {
          if (pending.connection === connectionRevision.current && eqPending.current === null) {
            setError(friendlyError(eqError, "change the equalizer"));
          }
        }
      }
    } finally {
      eqSending.current = false;
      if (eqPending.current) void drainEq();
    }
  };

  const applyEq = (next: [number, number, number, number, number]) => {
    gainsRef.current = next;
    setGains(next);
    if (!token) return;
    eqPending.current = {
      base,
      token,
      connection: connectionRevision.current,
      value: next,
    };
    void drainEq();
  };

  const serviceAvailable = status?.service_enabled !== false;
  const live = serviceAvailable && Boolean(activeOutput);

  const pairingScreen = (
    <PairingHome
      scanning={scanning}
      pairing={pairing}
      found={found}
      host={host}
      pin={pin}
      error={error}
      onScan={() => void scan()}
      onSelectHost={setDesktopHost}
      onChangeHost={setDesktopHost}
      onChangePin={(value) => setPin(value.replace(/\\D/g, "").slice(0, 6))}
      onPair={() => void pair()}
    />
  );

  const mixerScreen = (
    <MixerHome
      activeInput={activeInput ? prettyInput(activeInput) : ""}
      activeOutput={activeOutput?.device_name ?? ""}
      volume={volume}
      live={live}
      statusText={
        connectionState === "reconnecting"
          ? "Reconnecting"
          : connectionState === "connecting"
            ? "Connecting"
            : live
              ? "Live"
              : !serviceAvailable
                ? "Paused"
                : "Ready"
      }
      error={error}
      volumeDisabled={!serviceAvailable || connectionState !== "connected" || !activeOutput}
      soundDisabled={!serviceAvailable}
      onChangeInput={() => setSourceOpen(true)}
      onChangeOutput={() => setOutputOpen(true)}
      onOpenSound={() => router.push("/sound")}
      onOpenMore={() => router.push("/connection")}
      onDisconnect={disconnect}
      onVolumeChange={applyVolume}
      cd={cd}
      onCdPlayPause={() => void applyCd(cd.playing ? "pause" : "play")}
      onCdPrev={() => void applyCd("prev")}
      onCdNext={() => void applyCd("next")}
    />
  );

  const home = (
    <View style={{ flex: 1, backgroundColor: mobileColors.background }}>
      <StatusBar style="light" />
      {!hydrated ? (
        <Host style={{ flex: 1 }} colorScheme="dark">
          <FieldGroup testID="pairing-restore">
            <FieldGroup.Section title="on-air remote">
              <Text>Restoring your desktop connection…</Text>
            </FieldGroup.Section>
          </FieldGroup>
        </Host>
      ) : token ? (
        mixerScreen
      ) : (
        pairingScreen
      )}
      {token && sourceOpen && (
        <BottomSheet
          modifiers={sheetPresentation}
          isPresented
          onDismiss={() => setSourceOpen(false)}
          showDragIndicator
          snapPoints={Platform.OS === "ios" ? [{ height: 280 }] : undefined}
          testID="source-sheet"
        >
          <Column spacing={theme.spacing.md} style={{ padding: theme.spacing.md }}>
            <Row alignment="center" spacing={theme.spacing.sm}>
              <Text textStyle={{ fontSize: 22, fontWeight: "700" }}>Choose source</Text>
              <Spacer />
              <Button label="Done" variant="text" onPress={() => setSourceOpen(false)} />
            </Row>
            <Text>Select the Mac audio capture device to stream.</Text>
            <Picker
              selectedValue={activeInput}
              onValueChange={(name) => void pickInput(String(name))}
              appearance="menu"
              enabled={serviceAvailable && busyTarget === null && uniqueInputs.length > 0}
              testID="source-picker"
            >
              <Picker.Item label="Choose a source" value="" />
              {uniqueInputs.map((name) => (
                <Picker.Item key={name} label={prettyInput(name)} value={name} />
              ))}
            </Picker>
            {uniqueInputs.length === 0 && <Text>No capture devices are available.</Text>}
          </Column>
        </BottomSheet>
      )}
      {token && outputOpen && (
        <BottomSheet
          modifiers={sheetPresentation}
          isPresented
          onDismiss={() => setOutputOpen(false)}
          showDragIndicator
          snapPoints={
            Platform.OS === "ios"
              ? outputs.length > 5
                ? ["full"]
                : [{ height: Math.max(300, 160 + outputs.length * 78) }]
              : undefined
          }
          testID="output-sheet"
        >
          <Column spacing={theme.spacing.md} style={{ padding: theme.spacing.md }}>
            <Row alignment="center" spacing={theme.spacing.sm}>
              <Text textStyle={{ fontSize: 22, fontWeight: "700" }}>Choose speaker</Text>
              <Spacer />
              <Button label="Done" variant="text" onPress={() => setOutputOpen(false)} />
            </Row>
            {outputs.length === 0 && <Text>Waiting for speakers on the LAN…</Text>}
            {airplayMode === "avroute-picker" && (
              <Text>AirPlay selection is available from the desktop picker on macOS.</Text>
            )}
            <Button
              label="Add Bluetooth speaker"
              variant="outlined"
              disabled={!serviceAvailable || !token}
              onPress={() => {
                void (async () => {
                  try {
                    await openBluetoothSettings(base, token);
                    await refresh();
                    setTimeout(() => void refresh(), 2500);
                  } catch (settingsError) {
                    setError(friendlyError(settingsError, "open Bluetooth settings"));
                  }
                })();
              }}
              testID="add-bluetooth"
            />
            {outputs.map((output) => {
              const selected =
                activeOutput?.transport === output.transport &&
                activeOutput.device_id === output.id;
              const desktopOnly =
                output.transport === "airplay" && airplayMode === "avroute-picker";
              const working = busyTarget === `${output.transport}:${output.id}`;
              const pair = (output.member_count ?? 1) >= 2 || output.kind === "pair";
              const action = desktopOnly
                ? "Desktop only"
                : working
                  ? "Connecting…"
                  : selected
                    ? "Connected"
                    : output.needs_pair && !output.paired
                      ? "Pair"
                      : "Connect";
              return (
                <Button
                  key={`${output.transport}-${output.id}`}
                  variant={selected ? "filled" : "outlined"}
                  disabled={!serviceAvailable || busyTarget !== null || desktopOnly}
                  onPress={() => void chooseOutput(output)}
                  testID={`output-${output.transport}-${output.id}`}
                >
                  <Row alignment="center" spacing={theme.spacing.sm}>
                    <Column spacing={theme.spacing.xs}>
                      <Text textStyle={{ fontWeight: "600" }}>{output.name}</Text>
                      <Text>{`${pair ? "Stereo pair · " : ""}${output.transport}`}</Text>
                    </Column>
                    <Spacer />
                    <Text>{action}</Text>
                  </Row>
                </Button>
              );
            })}
          </Column>
        </BottomSheet>
      )}
      {pairTarget && (
        <BottomSheet
          modifiers={sheetPresentation}
          isPresented
          onDismiss={() => setPairTarget(null)}
          showDragIndicator
          snapPoints={
            Platform.OS === "ios"
              ? [{ height: pairTarget.transport === "airplay" ? 360 : 300 }]
              : undefined
          }
          testID="pair-sheet"
        >
          <Column spacing={theme.spacing.md} style={{ padding: theme.spacing.md }}>
            <Text textStyle={{ fontSize: 22, fontWeight: "700" }}>{`Pair ${pairTarget.name}`}</Text>
            <Text>
              {pairTarget.transport === "airplay"
                ? "Enter the code shown by the speaker. If no code appears, leave it blank."
                : "Confirm pairing on the speaker or in the computer’s Bluetooth settings, then continue."}
            </Text>
            {pairTarget.transport === "airplay" && (
              <TextInput
                value={devicePinInput}
                onChangeText={(value) => {
                  const next = value.replace(/\D/g, "").slice(0, 8);
                  devicePinInput.value = next;
                  setPairPin(next);
                }}
                placeholder="Speaker code"
                keyboardType="number-pad"
                inputMode="numeric"
                maxLength={8}
                testID="device-pin-input"
                style={{
                  padding: 12,
                  borderWidth: 1,
                  borderColor: theme.seedColor,
                  borderRadius: 12,
                }}
              />
            )}
            <Row alignment="center" spacing={theme.spacing.sm}>
              <Button
                label="Cancel"
                variant="text"
                disabled={devicePairing}
                onPress={() => setPairTarget(null)}
                testID="device-pair-cancel"
              />
              <Spacer />
              <Button
                label={devicePairing ? "Pairing…" : "Pair and connect"}
                disabled={devicePairing}
                onPress={() => void submitDevicePair()}
                testID="device-pair-submit"
              />
            </Row>
          </Column>
        </BottomSheet>
      )}
    </View>
  );
  return {
    home,
    paired: Boolean(token),
    host: normalizedHost,
    error,
    serviceAvailable: serviceAvailable && connectionState === "connected",
    servicePaused: !serviceAvailable,
    refreshing,
    connectionState,
    gains,
    sampleRate,
    outputSampleRate,
    inputRates,
    outputRates,
    configuringRate,
    applyEq,
    applyRate,
    refresh,
    disconnect,
    changeBand: (index: number, value: number) => {
      const next = [...gainsRef.current] as typeof gains;
      next[index] = value;
      applyEq(next);
    },
  };
}

const RemoteContext = createContext<ReturnType<typeof useRemoteController> | null>(null);

export function RemoteProvider({ children }: { children: React.ReactNode }) {
  const session = useRemoteController();
  return <RemoteContext value={session}>{children}</RemoteContext>;
}

export function useRemoteSession() {
  const session = use(RemoteContext);
  if (!session) throw new Error("RemoteProvider is required");
  return session;
}

export function RemoteHome() {
  return useRemoteSession().home;
}
