import { Button as ExpoButton, Host, Slider } from "@expo/ui";
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
import {
  ActivityIndicator,
  AppState,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
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

const EQ_LABELS = ["60", "250", "1k", "4k", "12k"] as const;
const colors = {
  well: "#0a0908",
  face: "#1a1814",
  face2: "#221f1a",
  ink: "#f3ead4",
  steel: "#c4b89a",
  steelDim: "#8a7f68",
  live: "#ff3b2a",
  amber: "#e0a24b",
  border: "#3a342a",
};

type PairTarget = { transport: string; id: string; name: string };

async function localIpv4(): Promise<string | undefined> {
  try {
    const Network = await import("expo-network");
    return await Network.getIpAddressAsync();
  } catch {
    return undefined;
  }
}

function Fader(props: {
  label: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  testId: string;
  onChange: (value: number) => void;
}) {
  const step = props.step ?? 1;
  const clamp = (n: number) => Math.min(props.max, Math.max(props.min, n));
  const [draft, setDraft] = useState(props.value);
  const commitTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const onChange = useRef(props.onChange);
  onChange.current = props.onChange;

  useEffect(() => setDraft(props.value), [props.value]);
  useEffect(
    () => () => {
      if (commitTimer.current) clearTimeout(commitTimer.current);
    },
    [],
  );

  const changeNow = (next: number) => {
    if (commitTimer.current) clearTimeout(commitTimer.current);
    setDraft(next);
    onChange.current(next);
  };
  const preview = (value: number) => {
    const next = clamp(value);
    setDraft(next);
    if (commitTimer.current) clearTimeout(commitTimer.current);
    // Avoid a LAN request for every drag sample on older phones and laptops.
    commitTimer.current = setTimeout(() => onChange.current(next), 180);
  };

  return (
    <View style={styles.fader} testID={props.testId}>
      <View style={styles.faderHeader}>
        <Text style={styles.faderLabel}>{props.label}</Text>
        <Text style={styles.faderValue}>{draft}</Text>
      </View>
      <View style={styles.faderControls}>
        <Pressable
          accessibilityLabel={`Decrease ${props.label}`}
          accessibilityRole="button"
          onPress={() => changeNow(clamp(draft - step))}
          style={styles.faderBtn}
          testID={`${props.testId}-down`}
        >
          <Text style={styles.faderBtnText}>−</Text>
        </Pressable>
        <Host matchContents style={styles.nativeSliderHost}>
          <Slider
            min={props.min}
            max={props.max}
            step={step}
            value={draft}
            onValueChange={preview}
            testID={`${props.testId}-native`}
          />
        </Host>
        <Pressable
          accessibilityLabel={`Increase ${props.label}`}
          accessibilityRole="button"
          onPress={() => changeNow(clamp(draft + step))}
          style={styles.faderBtn}
          testID={`${props.testId}-up`}
        >
          <Text style={styles.faderBtnText}>+</Text>
        </Pressable>
      </View>
    </View>
  );
}

export default function App(): React.JSX.Element {
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
  const [sampleRate, setInRate] = useState(44100);
  const [outputSampleRate, setOutRate] = useState(44100);
  const [inputRates, setInputRates] = useState<number[]>([44100, 48000]);
  const [outputRates, setOutputRates] = useState<number[]>([44100, 48000]);
  const [airplayMode, setAirplayMode] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [pairTarget, setPairTarget] = useState<PairTarget | null>(null);
  const [pairPin, setPairPin] = useState("");
  const [pairing, setPairing] = useState(false);
  const [devicePairing, setDevicePairing] = useState(false);
  const [configuringRate, setConfiguringRate] = useState(false);
  const [busyTarget, setBusyTarget] = useState<string | null>(null);
  const [appActive, setAppActive] = useState(AppState.currentState === "active");
  const didInitialScan = useRef(false);
  const scanInFlight = useRef(false);
  const refreshInFlight = useRef<Promise<void> | null>(null);
  const refreshQueued = useRef(false);

  const base = apiBase(host, DEFAULT_PORT);

  const scan = useCallback(async () => {
    if (scanInFlight.current) return;
    scanInFlight.current = true;
    setScanning(true);
    setError(null);
    try {
      const localIp = await localIpv4();
      const hits = await discoverOnAir({
        localIp,
        extraHosts: host ? [host] : [],
      });
      setFound(hits);
      if (hits[0] && host === "127.0.0.1") {
        setHost(hits[0].host);
      }
    } catch (err) {
      setError(String(err));
    } finally {
      scanInFlight.current = false;
      setScanning(false);
    }
  }, [host]);

  const refresh = useCallback((): Promise<void> => {
    if (!token) return Promise.resolve();
    if (refreshInFlight.current) {
      refreshQueued.current = true;
      return refreshInFlight.current;
    }

    const run = (async () => {
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
            getAirplayMode(base, token),
          ]);
          const [st, ins, outs, actIn, actOut, eq, sr, ap] = results;
          if (st.status === "fulfilled") setStatus(st.value);
          if (ins.status === "fulfilled") setInputs(ins.value);
          if (outs.status === "fulfilled") setOutputs(outs.value);
          if (actIn.status === "fulfilled") setActiveInput(actIn.value.name ?? "");
          if (actOut.status === "fulfilled") setActiveOutput(actOut.value);
          if (eq.status === "fulfilled") setGains(eq.value);
          if (sr.status === "fulfilled") {
            setInRate(sr.value.input.sample_rate_hz);
            setOutRate(sr.value.output.sample_rate_hz);
            if (sr.value.input.supported_hz.length) {
              setInputRates(sr.value.input.supported_hz);
            }
            if (sr.value.output.supported_hz.length) {
              setOutputRates(sr.value.output.supported_hz);
            }
          }
          if (ap.status === "fulfilled") setAirplayMode(ap.value);

          const failure = results.find((result) => result.status === "rejected");
          if (failure?.status === "rejected") throw failure.reason;
        } catch (err) {
          if (err instanceof HttpError && err.status === 401) {
            setToken(null);
            setStatus(null);
            setError("Pairing expired. Enter the current desktop PIN to reconnect.");
          } else if (err instanceof HttpError && err.status === 503) {
            setError("The desktop service is paused. Turn it on from the on-air tray menu.");
          } else {
            setError(String(err));
          }
          refreshQueued.current = false;
        }
      } while (refreshQueued.current);
    })();
    refreshInFlight.current = run;
    void run.finally(() => {
      if (refreshInFlight.current === run) refreshInFlight.current = null;
    });
    return run;
  }, [base, token]);

  useEffect(() => {
    if (didInitialScan.current) return;
    didInitialScan.current = true;
    void scan();
  }, [scan]);

  useEffect(() => {
    const subscription = AppState.addEventListener("change", (next) => {
      setAppActive(next === "active");
    });
    return () => subscription.remove();
  }, []);

  useEffect(() => {
    if (!token || !appActive) return;
    void refresh();
    const id = setInterval(() => void refresh(), 15000);
    let ws: WebSocket | undefined;
    try {
      ws = new WebSocket(wsUrl(base, token));
      ws.onmessage = (event) => {
        try {
          const message = JSON.parse(String(event.data)) as WsEvent;
          if (message.type !== "LevelMeter") void refresh();
        } catch {
          // Ignore malformed events; the periodic refresh remains a fallback.
        }
      };
    } catch {
      /* Expo web / tests may lack WS */
    }
    return () => {
      clearInterval(id);
      ws?.close();
    };
  }, [token, base, refresh, appActive]);

  const pair = async () => {
    const normalizedPin = pin.replace(/\D/g, "").slice(0, 6);
    if (pairing || normalizedPin.length !== 6) {
      setError("Enter the six-digit PIN shown by the desktop.");
      return;
    }
    setPairing(true);
    setError(null);
    try {
      const st = await fetchStatus(base);
      setStatus(st);
      const t = await verifyPin(base, normalizedPin);
      setToken(t);
    } catch (err) {
      setError(String(err));
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
    if (!token || busyTarget) return;
    setBusyTarget(`input:${name}`);
    setError(null);
    try {
      await activateInput(base, name, token);
      await refresh();
    } catch (err) {
      setError(String(err));
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
      setPairTarget({ transport: output.transport, id: output.id, name: output.name });
      return;
    }
    try {
      await activate(output);
    } catch (err) {
      setError(String(err));
    }
  };

  const submitPair = async () => {
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
        (o) => o.transport === pairTarget.transport && o.id === pairTarget.id,
      );
      setPairTarget(null);
      if (output) await activate(output);
    } catch (err) {
      setError(String(err));
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
    } catch (err) {
      if (kind === "input") setInRate(previous);
      else setOutRate(previous);
      setError(`Could not change the ${kind} sample rate: ${String(err)}`);
    } finally {
      setConfiguringRate(false);
    }
  };

  const disconnect = () => {
    setToken(null);
    setStatus(null);
    setInputs([]);
    setOutputs([]);
    setActiveInput("");
    setActiveOutput(null);
    setError(null);
  };

  const applyVolume = async (value: number) => {
    setVolumeValue(value);
    if (!token) return;
    try {
      await setVolume(base, value, token);
    } catch (err) {
      setError(String(err));
    }
  };

  const applyEq = async (next: [number, number, number, number, number]) => {
    setGains(next);
    if (!token) return;
    try {
      await setEq(base, next, token);
    } catch (err) {
      setError(String(err));
    }
  };

  const serviceAvailable = status?.service_enabled !== false;
  const live = serviceAvailable && Boolean(activeOutput);
  const lampLabel = live
    ? "on air"
    : !serviceAvailable
      ? "service paused"
      : error || !status
        ? "problem"
        : "ok";
  const lampColor = live
    ? colors.live
    : error || !status || !serviceAvailable
      ? colors.amber
      : "#5a1814";

  return (
    <SafeAreaView style={styles.container}>
      <StatusBar style="light" />
      <ScrollView contentInsetAdjustmentBehavior="automatic" contentContainerStyle={styles.scroll}>
        <View style={styles.header}>
          <View style={[styles.wordmark, live && styles.wordmarkLive]}>
            <Text style={[styles.wordmarkText, { color: lampColor }]} testID="core-status">
              ONAIR
            </Text>
            <Text style={styles.srOnly}>{status ? "ok" : "wait"}</Text>
          </View>
          <Text style={styles.title} testID="remote-title">
            on-air remote
          </Text>
          <Text style={styles.meta}>{lampLabel}</Text>
        </View>

        {!token && (
          <View style={styles.card}>
            <Text style={styles.heading}>Find desktop</Text>
            <Text style={styles.hint}>
              Scans the LAN for a running on-air mixer (_on-air._tcp / port {DEFAULT_PORT}).
            </Text>
            <Host matchContents style={styles.nativeButtonHost}>
              <ExpoButton
                label={scanning ? "Scanning…" : "Scan LAN"}
                onPress={() => void scan()}
                disabled={scanning}
                variant="outlined"
                testID="scan-button"
              />
            </Host>
            {scanning && <ActivityIndicator color={colors.amber} />}
            {found.map((hit) => (
              <Pressable
                key={`${hit.host}:${hit.port}`}
                style={[styles.row, host === hit.host && styles.rowOn]}
                onPress={() => setHost(hit.host)}
                testID={`discovered-${hit.host}`}
              >
                <Text style={styles.rowTitle}>{hit.name ?? "on-air"}</Text>
                <Text style={styles.rowMeta}>
                  {hit.host}:{hit.port}
                  {hit.version ? ` v${hit.version}` : ""}
                </Text>
              </Pressable>
            ))}
            <TextInput
              style={styles.input}
              testID="host-input"
              placeholder="Desktop LAN IP"
              placeholderTextColor={colors.steelDim}
              value={host}
              onChangeText={setHost}
              autoCapitalize="none"
              autoCorrect={false}
            />
            <TextInput
              style={styles.input}
              testID="pin-input"
              placeholder="Pairing PIN from the desktop"
              placeholderTextColor={colors.steelDim}
              value={pin}
              onChangeText={(value) => setPin(value.replace(/\D/g, "").slice(0, 6))}
              keyboardType="number-pad"
              maxLength={6}
            />
            <Host matchContents style={styles.nativeButtonHost}>
              <ExpoButton
                label={pairing ? "Pairing…" : "Pair"}
                onPress={() => void pair()}
                disabled={pairing || pin.length !== 6}
                testID="pair-button"
              />
            </Host>
          </View>
        )}

        {token && (
          <>
            <View style={styles.sessionRow}>
              <Text testID="paired-token" style={styles.paired} selectable>
                paired · {host}:{DEFAULT_PORT} · {airplayMode || "owntone"}
              </Text>
              <Pressable
                accessibilityRole="button"
                onPress={disconnect}
                style={styles.disconnectButton}
                testID="disconnect-button"
              >
                <Text style={styles.rowMeta}>Disconnect</Text>
              </Pressable>
            </View>
            {status && (
              <Text style={styles.meta}>
                core v{status.version} · {serviceAvailable ? "service on" : "service paused"}
              </Text>
            )}

            <Text style={styles.heading}>Source</Text>
            {uniqueInputs.length === 0 && <Text style={styles.hint}>No capture devices</Text>}
            {uniqueInputs.map((name) => (
              <Pressable
                key={name}
                testID={`input-${name}`}
                style={[styles.row, activeInput === name && styles.rowOn]}
                onPress={() => void pickInput(name)}
                disabled={busyTarget !== null}
              >
                <View style={[styles.dot, activeInput === name && styles.dotOn]} />
                <Text style={styles.rowTitle}>{prettyInput(name)}</Text>
              </Pressable>
            ))}

            <Text style={styles.heading}>Destination</Text>
            {airplayMode === "avroute-picker" && (
              <Text style={styles.hint}>
                macOS requires AirPlay selection on the desktop. Use the AirPlay picker in on-air.
              </Text>
            )}
            <Text testID="active-output" style={styles.srOnly}>
              {activeOutput ? `${activeOutput.transport}: ${activeOutput.device_name}` : "none"}
            </Text>
            {outputs.length === 0 && (
              <Text style={styles.hint}>Waiting for a speaker on the LAN</Text>
            )}
            {outputs.map((output) => {
              const on =
                activeOutput?.transport === output.transport &&
                activeOutput?.device_id === output.id;
              const desktopPickerOnly =
                output.transport === "airplay" && airplayMode === "avroute-picker";
              const pairMark = (output.member_count ?? 1) >= 2 || output.kind === "pair";
              return (
                <Pressable
                  key={`${output.transport}-${output.id}`}
                  testID={`output-${output.transport}-${output.id}`}
                  style={[styles.row, on && styles.rowOn, desktopPickerOnly && styles.disabled]}
                  onPress={() => void chooseOutput(output)}
                  disabled={busyTarget !== null || desktopPickerOnly}
                >
                  <View style={[styles.dot, on && styles.dotOn]} />
                  <Text style={styles.rowTitle}>{output.name}</Text>
                  <Text style={styles.rowMeta}>
                    {pairMark ? "pair · " : ""}
                    {desktopPickerOnly
                      ? "desktop picker"
                      : output.needs_pair && !output.paired
                        ? "pin"
                        : output.transport}
                  </Text>
                </Pressable>
              );
            })}

            <View style={styles.mixer}>
              <Fader
                label="level"
                value={volume}
                min={0}
                max={100}
                testId="volume-slider"
                onChange={(v) => void applyVolume(v)}
              />
              {gains.map((gain, i) => (
                <Fader
                  key={EQ_LABELS[i]}
                  label={EQ_LABELS[i]}
                  value={gain}
                  min={-12}
                  max={12}
                  step={0.5}
                  testId={`eq-band-${i}`}
                  onChange={(v) => {
                    const next = [...gains] as typeof gains;
                    next[i] = v;
                    void applyEq(next);
                  }}
                />
              ))}
            </View>
            <TextInput
              testID="volume-input"
              style={styles.hidden}
              value={String(volume)}
              onChangeText={(t) => void applyVolume(Number(t) || 0)}
            />

            <View style={styles.rates}>
              <Text style={styles.heading}>In</Text>
              <View style={styles.rateRow}>
                {inputRates.map((hz) => (
                  <Pressable
                    key={`in-${hz}`}
                    testID={hz === sampleRate ? "sample-rate" : `sample-rate-${hz}`}
                    style={[styles.chip, hz === sampleRate && styles.chipOn]}
                    disabled={configuringRate}
                    onPress={() => void applyRate("input", hz)}
                  >
                    <Text style={styles.chipText}>{hz}</Text>
                  </Pressable>
                ))}
              </View>
              <Text style={styles.heading}>Out</Text>
              <View style={styles.rateRow}>
                {outputRates.map((hz) => (
                  <Pressable
                    key={`out-${hz}`}
                    testID={
                      hz === outputSampleRate ? "output-sample-rate" : `output-sample-rate-${hz}`
                    }
                    style={[styles.chip, hz === outputSampleRate && styles.chipOn]}
                    disabled={configuringRate}
                    onPress={() => void applyRate("output", hz)}
                  >
                    <Text style={styles.chipText}>{hz}</Text>
                  </Pressable>
                ))}
              </View>
            </View>
          </>
        )}

        {error && (
          <Text style={styles.error} selectable accessibilityRole="alert">
            {error}
          </Text>
        )}
      </ScrollView>

      {pairTarget && (
        <View style={styles.sheet} testID="pair-sheet">
          <Text style={styles.heading}>Pair {pairTarget.name}</Text>
          <Text style={styles.hint}>
            {pairTarget.transport === "airplay"
              ? "Only if this speaker shows a code. HomePod mini usually has no PIN."
              : "Confirm pairing on the Bluetooth device, then continue."}
          </Text>
          {pairTarget.transport === "airplay" && (
            <TextInput
              style={styles.input}
              testID="device-pin-input"
              placeholder="PIN"
              placeholderTextColor={colors.steelDim}
              value={pairPin}
              onChangeText={setPairPin}
              keyboardType="number-pad"
            />
          )}
          <View style={styles.sheetActions}>
            <Pressable
              style={styles.button}
              testID="device-pair-cancel"
              disabled={devicePairing}
              onPress={() => setPairTarget(null)}
            >
              <Text style={styles.buttonText}>Cancel</Text>
            </Pressable>
            <Pressable
              style={[styles.buttonLive, devicePairing && styles.disabled]}
              testID="device-pair-submit"
              disabled={devicePairing}
              onPress={() => void submitPair()}
            >
              <Text style={styles.buttonLiveText}>
                {devicePairing ? "Pairing…" : "Pair & go live"}
              </Text>
            </Pressable>
          </View>
        </View>
      )}
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: colors.well },
  scroll: { padding: 16, gap: 8, paddingBottom: 48 },
  header: { alignItems: "flex-start", gap: 6, marginBottom: 8 },
  wordmark: {
    borderWidth: 2,
    borderColor: "#1a1612",
    backgroundColor: "#1a1814",
    paddingHorizontal: 12,
    paddingVertical: 6,
    borderRadius: 2,
  },
  wordmarkLive: { backgroundColor: "#2a0c0a" },
  wordmarkText: { fontSize: 18, fontWeight: "700", letterSpacing: 4 },
  title: { color: colors.ink, fontSize: 16, fontWeight: "600" },
  meta: { color: colors.steelDim, fontSize: 11, fontFamily: "monospace" },
  paired: { color: colors.amber, fontSize: 12, marginBottom: 4 },
  sessionRow: { flexDirection: "row", alignItems: "center", gap: 8 },
  disconnectButton: { marginLeft: "auto", paddingHorizontal: 8, paddingVertical: 8 },
  card: { gap: 8, marginBottom: 12 },
  heading: {
    color: colors.steelDim,
    fontSize: 10,
    letterSpacing: 2,
    textTransform: "uppercase",
    marginTop: 12,
    marginBottom: 6,
  },
  hint: { color: colors.steelDim, fontSize: 12, marginBottom: 6 },
  input: {
    borderWidth: 1,
    borderColor: colors.border,
    backgroundColor: colors.face,
    color: colors.ink,
    padding: 10,
    borderRadius: 6,
  },
  button: {
    borderWidth: 1,
    borderColor: colors.border,
    backgroundColor: colors.face2,
    padding: 12,
    borderRadius: 6,
    alignItems: "center",
  },
  buttonText: { color: colors.ink },
  buttonLive: {
    borderWidth: 1,
    borderColor: "#6a2a22",
    backgroundColor: "#3a1814",
    padding: 12,
    borderRadius: 6,
    alignItems: "center",
  },
  buttonLiveText: { color: "#ffd4cc", fontWeight: "600" },
  disabled: { opacity: 0.5 },
  row: {
    flexDirection: "row",
    alignItems: "center",
    gap: 8,
    borderWidth: 1,
    borderColor: "#322e26",
    backgroundColor: colors.face,
    padding: 12,
    borderRadius: 8,
    marginBottom: 6,
  },
  rowOn: { borderColor: "#6a2a22", backgroundColor: "#3a221c" },
  rowTitle: { color: colors.ink, flex: 1 },
  rowMeta: { color: colors.steelDim, fontSize: 10, textTransform: "uppercase" },
  dot: { width: 9, height: 9, borderRadius: 5, borderWidth: 1, borderColor: colors.steelDim },
  dotOn: { backgroundColor: colors.live, borderColor: colors.live },
  mixer: { gap: 6, marginTop: 16 },
  fader: { width: "100%", gap: 2 },
  faderHeader: { flexDirection: "row", justifyContent: "space-between", alignItems: "center" },
  faderControls: { flexDirection: "row", alignItems: "center", gap: 8 },
  faderValue: { color: colors.amber, fontSize: 12, fontFamily: "monospace" },
  faderLabel: { color: colors.steelDim, fontSize: 11, textTransform: "uppercase" },
  faderBtn: {
    width: 44,
    height: 44,
    alignItems: "center",
    justifyContent: "center",
    borderWidth: 1,
    borderColor: colors.border,
    borderRadius: 4,
  },
  faderBtnText: { color: colors.ink, fontSize: 20 },
  nativeSliderHost: { flex: 1, minHeight: 44 },
  nativeButtonHost: { width: "100%", minHeight: 44 },
  rates: { marginTop: 8 },
  rateRow: { flexDirection: "row", flexWrap: "wrap", gap: 8 },
  chip: {
    borderWidth: 1,
    borderColor: colors.border,
    paddingHorizontal: 10,
    paddingVertical: 6,
    borderRadius: 4,
  },
  chipOn: { borderColor: colors.amber },
  chipText: { color: colors.ink, fontFamily: "monospace", fontSize: 11 },
  error: { color: "#f0b4ac", marginTop: 12 },
  hidden: { height: 0, opacity: 0 },
  srOnly: { height: 0, opacity: 0 },
  sheet: {
    position: "absolute",
    left: 0,
    right: 0,
    bottom: 0,
    backgroundColor: colors.face2,
    borderTopWidth: 1,
    borderColor: colors.border,
    padding: 16,
    gap: 8,
  },
  sheetActions: { flexDirection: "row", gap: 8 },
});
