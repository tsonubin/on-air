import {
  BottomSheet,
  Button,
  Collapsible,
  Column,
  FieldGroup,
  Host,
  Picker,
  RNHostView,
  Row,
  Spacer,
  Text,
  TextInput,
  useNativeState,
} from "@expo/ui";
import type {
  ActiveOutput,
  DiscoveredHost,
  OutputInfo,
  StatusResponse,
  WsEvent,
} from "@on-air/api-types";
import { DEFAULT_PORT } from "@on-air/api-types";
import { StatusBar } from "expo-status-bar";
import type React from "react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AppState, Text as ReactNativeText, useColorScheme, View } from "react-native";
import {
  activateInput,
  activateOutput,
  apiBase,
  discoverOnAir,
  fetchStatus,
  getActiveInput,
  getActiveOutput,
  getAirplayMode,
  getEq,
  getSampleRate,
  getVolume,
  HttpError,
  listInputs,
  listOutputs,
  pairAirplay,
  pairBluetooth,
  prettyInput,
  setEq,
  setSampleRate,
  setVolume,
  verifyPin,
  wsUrl,
} from "./src/controlClient";
import { NativeFader } from "./src/native-fader";
import { clearPairing, loadPairing, savePairing } from "./src/pairing-store";
import { brandColors, theme } from "./src/theme";

const EQ_LABELS = ["60 Hz", "250 Hz", "1 kHz", "4 kHz", "12 kHz"] as const;

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

function formatRate(hz: number): string {
  if (hz % 1000 === 0) return `${hz / 1000} kHz`;
  return `${(hz / 1000).toFixed(1)} kHz`;
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

function ErrorNotice({ message }: { message: string }) {
  const scheme = useColorScheme();
  const color = brandColors[scheme === "dark" ? "dark" : "light"].error;
  return (
    <RNHostView matchContents>
      <ReactNativeText accessibilityRole="alert" selectable style={{ color, fontSize: 15 }}>
        {message}
      </ReactNativeText>
    </RNHostView>
  );
}

export default function App(): React.JSX.Element {
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
  const [error, setError] = useState<string | null>(null);
  const [pairTarget, setPairTarget] = useState<PairTarget | null>(null);
  const [pairPin, setPairPin] = useState("");
  const [pairing, setPairing] = useState(false);
  const [devicePairing, setDevicePairing] = useState(false);
  const [configuringRate, setConfiguringRate] = useState(false);
  const [busyTarget, setBusyTarget] = useState<string | null>(null);
  const [toneOpen, setToneOpen] = useState(false);
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [appActive, setAppActive] = useState(AppState.currentState === "active");
  const hostInput = useNativeState("127.0.0.1");
  const pinInput = useNativeState("");
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
    refreshQueued.current = false;
    volumePending.current = null;
    eqPending.current = null;
  }, []);

  const setDesktopHost = useCallback(
    (value: string) => {
      scanRevision.current += 1;
      hostInput.value = value;
      setHost(value);
    },
    [hostInput],
  );
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
      if (hits.length === 0) {
        setError("No desktop was found automatically. Enter its LAN address below.");
      }
    } catch (scanError) {
      if (scanRevision.current === revision) {
        setError(friendlyError(scanError, "scan the local network"));
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
    const run = (async () => {
      const connection = connectionRevision.current;
      do {
        refreshQueued.current = false;
        setError(null);
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
          ]);
          if (connectionRevision.current !== connection) {
            refreshQueued.current = false;
            return;
          }
          const [st, ins, outs, actIn, actOut, eq, sr, savedVolume, ap] = results;
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
          const failure = results.find((result) => result.status === "rejected");
          if (failure?.status === "rejected") throw failure.reason;
        } catch (refreshError) {
          if (connectionRevision.current !== connection) return;
          if (refreshError instanceof HttpError && refreshError.status === 401) {
            invalidateConnection();
            setToken(null);
            setStatus(null);
            void clearPairing().catch(() => {});
          }
          setError(friendlyError(refreshError, "refresh the mixer"));
          refreshQueued.current = false;
        }
      } while (refreshQueued.current);
    })();
    refreshInFlight.current = run;
    void run.finally(() => {
      if (refreshInFlight.current === run) refreshInFlight.current = null;
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
    let ws: WebSocket | undefined;
    try {
      ws = new WebSocket(wsUrl(base, token));
      ws.onmessage = (event) => {
        try {
          const message = JSON.parse(String(event.data)) as WsEvent;
          if (message.type === "ServiceStateChanged") {
            setStatus((current) =>
              current ? { ...current, service_enabled: message.enabled } : current,
            );
          } else if (message.type !== "LevelMeter") {
            if (eventRefresh) clearTimeout(eventRefresh);
            eventRefresh = setTimeout(() => void refresh(), 300);
          }
        } catch {
          // Periodic refresh remains the recovery path for malformed events.
        }
      };
    } catch {
      // Expo web and unit tests may not provide WebSocket.
    }
    return () => {
      clearInterval(id);
      if (eventRefresh) clearTimeout(eventRefresh);
      ws?.close();
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
      invalidateConnection();
      setStatus(nextStatus);
      setToken(nextToken);
      void savePairing({ host: desktopHost, token: nextToken }).catch(() => {});
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
    } finally {
      setBusyTarget(null);
    }
  };

  const chooseOutput = async (output: OutputInfo) => {
    if (output.needs_pair && !output.paired) {
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
    setError(null);
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
  const statusLabel = live
    ? "● Live"
    : !serviceAvailable
      ? "Service paused"
      : status
        ? "Ready"
        : "Connecting";

  const pairingScreen = (
    <FieldGroup testID="pairing-screen">
      <FieldGroup.Section title="on-air remote">
        <Column spacing={theme.spacing.sm}>
          <Text testID="remote-title" textStyle={{ fontSize: 24, fontWeight: "700" }}>
            on-air remote
          </Text>
          <Text testID="core-status" textStyle={{ fontSize: 17, fontWeight: "600" }}>
            ONAIR
          </Text>
          <Text>Find the desktop mixer, enter its pairing code, and start listening.</Text>
        </Column>
      </FieldGroup.Section>
      <FieldGroup.Section title="Find desktop">
        <Button
          label={scanning ? "Scanning local network…" : "Scan LAN"}
          disabled={scanning}
          onPress={() => void scan()}
          testID="scan-button"
        />
        {found.map((hit) => {
          const selected = normalizeDesktopHost(host) === hit.host;
          return (
            <Button
              key={`${hit.host}:${hit.port}`}
              variant={selected ? "filled" : "outlined"}
              onPress={() => setDesktopHost(hit.host)}
              testID={`discovered-${hit.host}`}
            >
              <Row alignment="center" spacing={theme.spacing.sm}>
                <Column spacing={theme.spacing.xs}>
                  <Text textStyle={{ fontWeight: "600" }}>{hit.name ?? "on-air desktop"}</Text>
                  <Text>{`${hit.host}:${hit.port}`}</Text>
                </Column>
                <Spacer />
                <Text>{selected ? "Selected" : "Choose"}</Text>
              </Row>
            </Button>
          );
        })}
        {!scanning && found.length === 0 && (
          <Text>No mixer found yet. Automatic scan and manual address both work in Expo Go.</Text>
        )}
      </FieldGroup.Section>
      <FieldGroup.Section title="Connect manually">
        <Column spacing={theme.spacing.sm}>
          <Text textStyle={{ fontWeight: "600" }}>Desktop LAN address</Text>
          <TextInput
            value={hostInput}
            onChangeText={setDesktopHost}
            placeholder="192.168.1.20"
            autoCapitalize="none"
            autoCorrect={false}
            keyboardType="url"
            returnKeyType="next"
            testID="host-input"
            style={{
              padding: 12,
              borderWidth: 1,
              borderColor: theme.seedColor,
              borderRadius: 12,
            }}
          />
          <Text textStyle={{ fontWeight: "600" }}>Six-digit pairing code</Text>
          <TextInput
            value={pinInput}
            onChangeText={(value) => {
              const next = value.replace(/\D/g, "").slice(0, 6);
              pinInput.value = next;
              setPin(next);
            }}
            placeholder="000000"
            keyboardType="number-pad"
            inputMode="numeric"
            maxLength={6}
            returnKeyType="done"
            onSubmitEditing={() => void pair()}
            testID="pin-input"
            style={{
              padding: 12,
              borderWidth: 1,
              borderColor: theme.seedColor,
              borderRadius: 12,
            }}
            textStyle={{ fontSize: 22, fontWeight: "600", letterSpacing: 4, textAlign: "center" }}
          />
          <Button
            label={pairing ? "Pairing…" : "Pair with desktop"}
            disabled={pairing || pin.length !== 6 || !normalizeDesktopHost(host)}
            onPress={() => void pair()}
            testID="pair-button"
          />
          <Text>
            The code is shown only on the desktop app. Both devices must use the same LAN.
          </Text>
          {error && <ErrorNotice message={error} />}
        </Column>
      </FieldGroup.Section>
    </FieldGroup>
  );

  const mixerScreen = (
    <FieldGroup testID="mixer-screen">
      <FieldGroup.Section title="on-air remote">
        <Column spacing={theme.spacing.sm}>
          <Row alignment="center" spacing={theme.spacing.sm}>
            <Text testID="remote-title" textStyle={{ fontSize: 24, fontWeight: "700" }}>
              on-air remote
            </Text>
            <Spacer />
            <Text testID="core-status" textStyle={{ fontWeight: "700" }}>
              {statusLabel}
            </Text>
          </Row>
          <Text testID="paired-token">{`paired · ${normalizedHost}:${DEFAULT_PORT}`}</Text>
          <Text>{status ? `Core ${status.version}` : "Waiting for desktop status"}</Text>
          <Row alignment="center" spacing={theme.spacing.sm}>
            <Button
              label="Refresh"
              variant="outlined"
              onPress={() => void refresh()}
              testID="refresh-button"
            />
            <Spacer />
            <Button
              label="Disconnect"
              variant="text"
              onPress={disconnect}
              testID="disconnect-button"
            />
          </Row>
          {!serviceAvailable && (
            <Text>Turn the service on from the desktop tray before changing audio controls.</Text>
          )}
          {error && <ErrorNotice message={error} />}
        </Column>
      </FieldGroup.Section>
      <FieldGroup.Section title="Now playing">
        <Column spacing={theme.spacing.sm}>
          <Text textStyle={{ fontWeight: "600" }}>Source</Text>
          <Text>{activeInput ? prettyInput(activeInput) : "Choose an input below"}</Text>
          <Text textStyle={{ fontWeight: "600" }}>Speaker</Text>
          <Text testID="active-output">
            {activeOutput ? activeOutput.device_name : "Choose a destination below"}
          </Text>
        </Column>
      </FieldGroup.Section>
      <FieldGroup.Section title="Source" disabled={!serviceAvailable}>
        <Column spacing={theme.spacing.sm}>
          <Text>Capture device</Text>
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
      </FieldGroup.Section>
      <FieldGroup.Section title="Destination" disabled={!serviceAvailable}>
        {outputs.length === 0 && <Text>Waiting for speakers on the LAN…</Text>}
        {airplayMode === "avroute-picker" && (
          <Text>AirPlay selection is available from the desktop picker on macOS.</Text>
        )}
        {outputs.map((output) => {
          const selected =
            activeOutput?.transport === output.transport && activeOutput.device_id === output.id;
          const desktopOnly = output.transport === "airplay" && airplayMode === "avroute-picker";
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
      </FieldGroup.Section>
      <FieldGroup.Section title="Volume" disabled={!serviceAvailable || !activeOutput}>
        <NativeFader
          label="Volume"
          value={volume}
          min={0}
          max={100}
          testId="volume-slider"
          disabled={!serviceAvailable || !activeOutput}
          showStepButtons
          onChange={(value) => void applyVolume(value)}
        />
      </FieldGroup.Section>
      <FieldGroup.Section title="Sound" disabled={!serviceAvailable}>
        <Collapsible
          label="Equalizer"
          isOpen={toneOpen}
          onOpenChange={setToneOpen}
          labelStyle={{ fontWeight: "600" }}
        >
          <Column spacing={theme.spacing.md}>
            {gains.map((gain, index) => (
              <NativeFader
                key={EQ_LABELS[index]}
                label={EQ_LABELS[index]}
                value={gain}
                min={-12}
                max={12}
                step={0.5}
                testId={`eq-band-${index}`}
                disabled={!serviceAvailable}
                onChange={(value) => {
                  const next = [...gainsRef.current] as typeof gains;
                  next[index] = value;
                  void applyEq(next);
                }}
              />
            ))}
            <Button
              label="Reset equalizer"
              variant="outlined"
              disabled={!serviceAvailable || gains.every((gain) => gain === 0)}
              onPress={() => void applyEq([0, 0, 0, 0, 0])}
              testID="eq-reset-button"
            />
          </Column>
        </Collapsible>
      </FieldGroup.Section>
      <FieldGroup.Section title="Audio quality" disabled={!serviceAvailable}>
        <Collapsible
          label="Sample rates"
          isOpen={advancedOpen}
          onOpenChange={setAdvancedOpen}
          labelStyle={{ fontWeight: "600" }}
        >
          <Column spacing={theme.spacing.md}>
            <Column spacing={theme.spacing.sm}>
              <Text textStyle={{ fontWeight: "600" }}>Input rate</Text>
              <Picker
                selectedValue={sampleRate}
                onValueChange={(value) => void applyRate("input", Number(value))}
                appearance="menu"
                enabled={serviceAvailable && !configuringRate}
                testID="sample-rate-picker"
              >
                {inputRates.map((hz) => (
                  <Picker.Item key={`in-${hz}`} label={formatRate(hz)} value={hz} />
                ))}
              </Picker>
            </Column>
            <Column spacing={theme.spacing.sm}>
              <Text textStyle={{ fontWeight: "600" }}>Output rate</Text>
              <Picker
                selectedValue={outputSampleRate}
                onValueChange={(value) => void applyRate("output", Number(value))}
                appearance="menu"
                enabled={serviceAvailable && !configuringRate}
                testID="output-sample-rate-picker"
              >
                {outputRates.map((hz) => (
                  <Picker.Item key={`out-${hz}`} label={formatRate(hz)} value={hz} />
                ))}
              </Picker>
            </Column>
          </Column>
        </Collapsible>
      </FieldGroup.Section>
    </FieldGroup>
  );

  return (
    <View style={{ flex: 1 }}>
      <StatusBar style="auto" />
      <Host style={{ flex: 1 }} useViewportSizeMeasurement seedColor={theme.seedColor}>
        {!hydrated ? (
          <FieldGroup testID="pairing-restore">
            <FieldGroup.Section title="on-air remote">
              <Text>Restoring your desktop connection…</Text>
            </FieldGroup.Section>
          </FieldGroup>
        ) : token ? (
          mixerScreen
        ) : (
          pairingScreen
        )}
        {pairTarget && (
          <BottomSheet
            isPresented
            onDismiss={() => setPairTarget(null)}
            showDragIndicator
            testID="pair-sheet"
          >
            <Column spacing={theme.spacing.md} style={{ padding: theme.spacing.md }}>
              <Text textStyle={{ fontSize: 22, fontWeight: "700" }}>
                {`Pair ${pairTarget.name}`}
              </Text>
              <Text>
                {pairTarget.transport === "airplay"
                  ? "Enter the code shown by the speaker. If no code appears, leave it blank."
                  : "Confirm pairing on the Bluetooth device, then continue."}
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
      </Host>
    </View>
  );
}
