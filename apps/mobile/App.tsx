import React, { useState } from "react";
import {
  SafeAreaView,
  Text,
  TextInput,
  Button,
  StyleSheet,
} from "react-native";
import { DEFAULT_PORT, type StatusResponse } from "@on-air/api-types";

function App(): React.JSX.Element {
  const [ip, setIp] = useState("");
  const [status, setStatus] = useState<StatusResponse | null>(null);
  const [error, setError] = useState<string | null>(null);

  const checkStatus = async () => {
    setError(null);
    setStatus(null);
    try {
      const response = await fetch(`http://${ip}:${DEFAULT_PORT}/api/status`);
      const json: StatusResponse = await response.json();
      setStatus(json);
    } catch (err) {
      setError(String(err));
    }
  };

  return (
    <SafeAreaView style={styles.container}>
      <Text style={styles.title}>on-air remote</Text>
      <TextInput
        style={styles.input}
        placeholder="Desktop LAN IP, e.g. 192.168.1.42"
        value={ip}
        onChangeText={setIp}
        autoCapitalize="none"
      />
      <Button title="Check status" onPress={checkStatus} />
      {status && (
        <Text>{`core status: ${status.status} (v${status.version})`}</Text>
      )}
      {error && <Text style={styles.error}>{error}</Text>}
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, padding: 16, gap: 12 },
  title: { fontSize: 20, fontWeight: "600" },
  input: { borderWidth: 1, borderColor: "#999", padding: 8, borderRadius: 6 },
  error: { color: "red" },
});

export default App;
