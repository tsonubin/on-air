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
  activateOutput,
  apiBase,
  fetchStatus,
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
  const [outputs, setOutputs] = useState<OutputInfo[]>([]);
  const [error, setError] = useState<string | null>(null);

  const base = apiBase(ip, DEFAULT_PORT);

  const pair = async () => {
    setError(null);
    try {
      const st = await fetchStatus(base);
      setStatus(st);
      const t = await verifyPin(base, pin);
      setToken(t);
      setOutputs(await listOutputs(base, t));
    } catch (err) {
      setError(String(err));
    }
  };

  const pick = async (output: OutputInfo) => {
    setError(null);
    try {
      await activateOutput(base, output.transport, output.id);
      await setVolume(base, 20);
      await setEq(base, [0, 0, 0, 0, 0]);
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
        {outputs.map((output) => (
          <Button
            key={`${output.transport}-${output.id}`}
            testID={`output-${output.transport}`}
            title={`${output.transport}: ${output.name}`}
            onPress={() => void pick(output)}
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
  input: { borderWidth: 1, borderColor: "#999", padding: 8, borderRadius: 6, marginVertical: 8 },
  error: { color: "red" },
});

export default App;
