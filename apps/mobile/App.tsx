import React, { useCallback, useEffect, useMemo, useState } from "react";
import {
  ActivityIndicator,
  Pressable,
  SafeAreaView,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  View,
} from "react-native";
import type { ActiveOutput, DiscoveredHost, OutputInfo, StatusResponse } from "@on-air/api-types";
import { DEFAULT_PORT } from "@on-air/api-types";
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
  const fill = ((props.value - props.min) / (props.max - props.min)) * 100;
  return (
    <View style={styles.fader} testID={props.testId}>
      <Text style={styles.faderValue}>{props.value}</Text>
      <Pressable onPress={() => props.onChange(clamp(props.value + step))} style={styles.faderBtn}>
        <Text style={styles.faderBtnText}>+</Text>
      </Pressable>
      <View style={styles.faderTrack}>
        <View style={[styles.faderFill, { height: (fill / 100) * 80 }]} />
      </View>
      <Pressable onPress={() => props.onChange(clamp(props.value - step))} style={styles.faderBtn}>
        <Text style={styles.faderBtnText}>−</Text>
      </Pressable>
      <Text style={styles.faderLabel}>{props.label}</Text>
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

  const base = apiBase(host, DEFAULT_PORT);

  const scan = useCallback(async () => {
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
      setScanning(false);
    }
  }, [host]);

  const refresh = useCallback(async () => {
    if (!token) return;
    setError(null);
    try {
      const [st, ins, outs, actIn, actOut, eq, sr, ap] = await Promise.all([
        fetchStatus(base),
        listInputs(base, token),
        listOutputs(base, token),
        getActiveInput(base, token),
        getActiveOutput(base, token),
        getEq(base, token),
        getSampleRate(base, token),
        getAirplayMode(base, token),
      ]);
      setStatus(st);
      setInputs(ins);
      setOutputs(outs);
      setActiveInput(actIn.name ?? "");
      setActiveOutput(actOut);
      setGains(eq);
      setInRate(sr.input.sample_rate_hz);
      setOutRate(sr.output.sample_rate_hz);
      if (sr.input.supported_hz.length) setInputRates(sr.input.supported_hz);
      if (sr.output.supported_hz.length) setOutputRates(sr.output.supported_hz);
      setAirplayMode(ap);
    } catch (err) {
      setError(String(err));
    }
  }, [base, token]);

  useEffect(() => {
    void scan();
  }, []);

  useEffect(() => {
    if (!token) return;
    void refresh();
    const id = setInterval(() => void refresh(), 4000);
    let ws: WebSocket | undefined;
    try {
      ws = new WebSocket(wsUrl(base, token));
      ws.onmessage = () => {
        void refresh();
      };
    } catch {
      /* Expo web / tests may lack WS */
    }
    return () => {
      clearInterval(id);
      ws?.close();
    };
  }, [token, base, refresh]);

  const pair = async () => {
    setError(null);
    try {
      const st = await fetchStatus(base);
      setStatus(st);
      const t = await verifyPin(base, pin);
      setToken(t);
    } catch (err) {
      setError(String(err));
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
    if (!token) return;
    try {
      await activateInput(base, name, token);
      await refresh();
    } catch (err) {
      setError(String(err));
    }
  };

  const activate = async (output: OutputInfo) => {
    if (!token) return;
    await activateOutput(base, output.transport, output.id, token);
    await refresh();
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
    if (!pairTarget || !token) return;
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
    }
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

  const live = Boolean(activeOutput);
  const lampLabel = live ? "on air" : error || !status ? "problem" : "ok";
  const lampColor = live ? colors.live : error || !status ? colors.amber : "#5a1814";

  return (
    <SafeAreaView style={styles.container}>
      <ScrollView contentContainerStyle={styles.scroll}>
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
            <Pressable style={styles.button} onPress={() => void scan()} testID="scan-button">
              <Text style={styles.buttonText}>{scanning ? "Scanning…" : "Scan LAN"}</Text>
            </Pressable>
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
              onChangeText={setPin}
              keyboardType="number-pad"
            />
            <Pressable style={styles.buttonLive} onPress={() => void pair()} testID="pair-button">
              <Text style={styles.buttonLiveText}>Pair</Text>
            </Pressable>
          </View>
        )}

        {token && (
          <>
            <Text testID="paired-token" style={styles.paired}>
              paired · {host}:{DEFAULT_PORT} · {airplayMode || "owntone"}
            </Text>
            {status && (
              <Text style={styles.meta}>
                core v{status.version}
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
              >
                <View style={[styles.dot, activeInput === name && styles.dotOn]} />
                <Text style={styles.rowTitle}>{prettyInput(name)}</Text>
              </Pressable>
            ))}

            <Text style={styles.heading}>Destination</Text>
            <Text testID="active-output" style={styles.srOnly}>
              {activeOutput ? `${activeOutput.transport}: ${activeOutput.device_name}` : "none"}
            </Text>
            {outputs.length === 0 && <Text style={styles.hint}>Waiting for a speaker on the LAN</Text>}
            {outputs.map((output) => {
              const on =
                activeOutput?.transport === output.transport && activeOutput?.device_id === output.id;
              const pairMark = (output.member_count ?? 1) >= 2 || output.kind === "pair";
              return (
                <Pressable
                  key={`${output.transport}-${output.id}`}
                  testID={`output-${output.transport}-${output.id}`}
                  style={[styles.row, on && styles.rowOn]}
                  onPress={() => void chooseOutput(output)}
                >
                  <View style={[styles.dot, on && styles.dotOn]} />
                  <Text style={styles.rowTitle}>{output.name}</Text>
                  <Text style={styles.rowMeta}>
                    {pairMark ? "pair · " : ""}
                    {output.needs_pair && !output.paired ? "pin" : output.transport}
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
                  key={i}
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
                    onPress={() => {
                      setInRate(hz);
                      void setSampleRate(base, { input_hz: hz }, token ?? undefined);
                    }}
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
                    testID={hz === outputSampleRate ? "output-sample-rate" : `output-sample-rate-${hz}`}
                    style={[styles.chip, hz === outputSampleRate && styles.chipOn]}
                    onPress={() => {
                      setOutRate(hz);
                      void setSampleRate(base, { output_hz: hz }, token ?? undefined);
                    }}
                  >
                    <Text style={styles.chipText}>{hz}</Text>
                  </Pressable>
                ))}
              </View>
            </View>
          </>
        )}

        {error && <Text style={styles.error}>{error}</Text>}
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
              placeholder="PIN"
              placeholderTextColor={colors.steelDim}
              value={pairPin}
              onChangeText={setPairPin}
              keyboardType="number-pad"
            />
          )}
          <View style={styles.sheetActions}>
            <Pressable style={styles.button} onPress={() => setPairTarget(null)}>
              <Text style={styles.buttonText}>Cancel</Text>
            </Pressable>
            <Pressable style={styles.buttonLive} onPress={() => void submitPair()}>
              <Text style={styles.buttonLiveText}>Pair & go live</Text>
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
  mixer: { flexDirection: "row", justifyContent: "space-around", marginTop: 16 },
  fader: { alignItems: "center", width: 44, gap: 4 },
  faderValue: { color: colors.amber, fontSize: 10, fontFamily: "monospace" },
  faderLabel: { color: colors.steelDim, fontSize: 9, textTransform: "uppercase" },
  faderBtn: {
    width: 28,
    height: 22,
    alignItems: "center",
    justifyContent: "center",
    borderWidth: 1,
    borderColor: colors.border,
    borderRadius: 4,
  },
  faderBtnText: { color: colors.ink },
  faderTrack: {
    width: 8,
    height: 80,
    backgroundColor: "#0c0b0a",
    borderRadius: 4,
    justifyContent: "flex-end",
    overflow: "hidden",
  },
  faderFill: { width: "100%", backgroundColor: "#5c4a32", borderRadius: 4 },
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
