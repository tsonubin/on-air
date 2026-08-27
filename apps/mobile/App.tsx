import React, { useState } from "react";
import {
  SafeAreaView,
  Text,
  TextInput,
  Button,
  StyleSheet,
  ScrollView,
} from "react-native";
import { DEFAULT_PORT, type OutputInfo, type StatusResponse } from "@on-air/api-types";
import {
  activateInput,
  activateOutput,
  apiBase,
  fetchStatus,
  listInputs,
  listOutputs,
  setEq,
  setVolume,
  verifyPin,
} from "./src/controlClient";

function App(): React.JSX.Element {
  const [ip, setIp] = useState("127.0.0.1");
  const [pin, setPin] = useState("");
  const [token, setToken] = useState<string | null>(null);
  const [status, setStatus] = useState<StatusResponse | null>(null);
  const [inputs, setInputs] = useState<string[]>([]);
  const [outputs, setOutputs] = useState<OutputInfo[]>([]);
  const [volume, setVolumeValue] = useState(50);
  const [gains, setGains] = useState<[number, number, number, number, number]>([
    0, 0, 0, 0, 0,
  ]);
  const [error, setError] = useState<string | null>(null);

  const base = apiBase(ip, DEFAULT_PORT);

  const pair = async () => {
    setError(null);
    try {
      const st = await fetchStatus(base);
      setStatus(st);
      const t = await verifyPin(base, pin);
      setToken(t);
      setInputs(await listInputs(base, t));
      setOutputs(await listOutputs(base, t));
    } catch (err) {
      setError(String(err));
    }
  };

  const pickSource = async (name: string) => {
    if (!token) return;
    setError(null);
    try {
      await activateInput(base, name, token);
    } catch (err) {
      setError(String(err));
    }
  };

  const pickOutput = async (output: OutputInfo) => {
    if (!token) return;
    setError(null);
    try {
      await activateOutput(base, output.transport, output.id, token);
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

  return (
    <SafeAreaView style={styles.container}>
      <ScrollView>
        <Text style={styles.title} testID="remote-title">
          on-air remote
        </Text>
        <Text>mDNS: look for _on-air._tcp, or enter IP</Text>
        <TextInput
          style={styles.input}
          testID="host-input"
          placeholder="Desktop LAN IP"
          value={ip}
          onChangeText={setIp}
          autoCapitalize="none"
        />
        <TextInput
          style={styles.input}
          testID="pin-input"
          placeholder="Pairing PIN"
          value={pin}
          onChangeText={setPin}
          keyboardType="number-pad"
        />
        <Button testID="pair-button" title="Pair" onPress={() => void pair()} />
        {token && <Text testID="paired-token">paired</Text>}
        {status && (
          <Text testID="core-status">{`core status: ${status.status} (v${status.version})`}</Text>
        )}
        <Text style={styles.heading}>Source</Text>
        {inputs.map((name) => (
          <Button
            key={name}
            testID={`input-${name}`}
            title={name}
            onPress={() => void pickSource(name)}
          />
        ))}
        <Text style={styles.heading}>Output</Text>
        {outputs.map((output) => (
          <Button
            key={`${output.transport}-${output.id}`}
            testID={`output-${output.transport}`}
            title={`${output.transport}: ${output.name}`}
            onPress={() => void pickOutput(output)}
          />
        ))}
        <Text style={styles.heading}>Volume {volume}</Text>
        <TextInput
          testID="volume-input"
          style={styles.input}
          keyboardType="number-pad"
          value={String(volume)}
          onChangeText={(t) => void applyVolume(Number(t) || 0)}
        />
        <Text style={styles.heading}>EQ</Text>
        {gains.map((gain, i) => (
          <TextInput
            key={i}
            testID={`eq-band-${i}`}
            style={styles.input}
            keyboardType="numeric"
            value={String(gain)}
            onChangeText={(t) => {
              const next = [...gains] as typeof gains;
              next[i] = Number(t) || 0;
              void applyEq(next);
            }}
          />
        ))}
        {error && <Text style={styles.error}>{error}</Text>}
      </ScrollView>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, padding: 16, gap: 12 },
  title: { fontSize: 20, fontWeight: "600" },
  heading: { fontSize: 16, fontWeight: "600", marginTop: 12 },
  input: { borderWidth: 1, borderColor: "#999", padding: 8, borderRadius: 6, marginVertical: 8 },
  error: { color: "red" },
});

export default App;
