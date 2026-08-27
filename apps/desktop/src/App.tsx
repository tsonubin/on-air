import { useCallback, useEffect, useState } from "react";
import "./App.css";
import {
  API_BASE,
  DEFAULT_PORT,
  type ActiveOutput,
  type OutputInfo,
  type StatusResponse,
} from "@on-air/api-types";

async function api<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`${API_BASE}${path}`, init);
  if (!response.ok) {
    throw new Error(`${path} ${response.status}`);
  }
  if (response.status === 204) {
    return undefined as T;
  }
  return response.json() as Promise<T>;
}

function App() {
  const [status, setStatus] = useState<StatusResponse | null>(null);
  const [inputs, setInputs] = useState<string[]>([]);
  const [outputs, setOutputs] = useState<OutputInfo[]>([]);
  const [activeInput, setActiveInput] = useState<string>("");
  const [activeOutput, setActiveOutput] = useState<ActiveOutput | null>(null);
  const [volume, setVolume] = useState(50);
  const [gains, setGains] = useState<[number, number, number, number, number]>([
    0, 0, 0, 0, 0,
  ]);
  const [sampleRate, setSampleRate] = useState(44100);
  const [pin, setPin] = useState("");
  const [airplayMode, setAirplayMode] = useState("");
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setError(null);
    try {
      const [st, ins, outs, actIn, actOut, eq, sr, pairing, ap] =
        await Promise.all([
          api<StatusResponse>("/api/status"),
          api<{ inputs: string[] }>("/api/inputs"),
          api<{ outputs: OutputInfo[] }>("/api/outputs"),
          api<{ name: string | null; backend: string }>("/api/inputs/active"),
          api<ActiveOutput | null>("/api/outputs/active"),
          api<{ gains_db: [number, number, number, number, number] }>("/api/eq"),
          api<{ sample_rate_hz: number }>("/api/sample-rate"),
          api<{ pin: string }>("/api/pairing/pin"),
          api<{ mode: string }>("/api/airplay/mode"),
        ]);
      setStatus(st);
      setInputs(ins.inputs);
      setOutputs(outs.outputs);
      setActiveInput(actIn.name ?? "");
      setActiveOutput(actOut);
      setGains(eq.gains_db);
      setSampleRate(sr.sample_rate_hz);
      setPin(pairing.pin);
      setAirplayMode(ap.mode);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const pickInput = async (name: string) => {
    await api("/api/inputs/active", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ name }),
    });
    await refresh();
  };

  const activate = async (output: OutputInfo) => {
    await api("/api/outputs/active", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ transport: output.transport, device_id: output.id }),
    });
    await refresh();
  };

  const applyVolume = async (value: number) => {
    setVolume(value);
    await api("/api/outputs/active/volume", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ volume: value }),
    });
  };

  const applyEq = async (next: [number, number, number, number, number]) => {
    setGains(next);
    await api("/api/eq", {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ gains_db: next }),
    });
  };

  return (
    <main className="app">
      <header>
        <h1>on-air</h1>
        <p data-testid="core-status">
          {status
            ? `core status: ${status.status} (v${status.version}) :${DEFAULT_PORT}`
            : "loading core status..."}
        </p>
        <p data-testid="pairing-pin">Mobile PIN: {pin || "…"}</p>
        <p data-testid="airplay-mode">AirPlay mode: {airplayMode}</p>
        <p data-testid="autostart-hint">Autostart: login item (plugin)</p>
      </header>
      {error && <p className="error">Error: {error}</p>}

      <section>
        <h2>Input</h2>
        <ul data-testid="input-list">
          {inputs.map((name) => (
            <li key={name}>
              <button
                data-testid={`input-${name}`}
                onClick={() => void pickInput(name)}
              >
                {name}
                {activeInput === name ? " (active)" : ""}
              </button>
            </li>
          ))}
        </ul>
      </section>

      <section>
        <h2>Output (exclusive)</h2>
        <p data-testid="active-output">
          {activeOutput
            ? `${activeOutput.transport}: ${activeOutput.device_name}`
            : "none"}
        </p>
        <ul data-testid="output-list">
          {outputs.map((output) => (
            <li key={`${output.transport}-${output.id}`}>
              <button
                data-testid={`output-${output.transport}-${output.id}`}
                onClick={() => void activate(output)}
              >
                {output.transport} — {output.name}
              </button>
            </li>
          ))}
        </ul>
      </section>

      <section>
        <h2>Volume</h2>
        <input
          data-testid="volume-slider"
          type="range"
          min={0}
          max={100}
          value={volume}
          onChange={(e) => void applyVolume(Number(e.target.value))}
        />
      </section>

      <section>
        <h2>EQ</h2>
        {gains.map((gain, i) => (
          <label key={i}>
            band {i}
            <input
              data-testid={`eq-band-${i}`}
              type="range"
              min={-12}
              max={12}
              step={0.5}
              value={gain}
              onChange={(e) => {
                const next = [...gains] as typeof gains;
                next[i] = Number(e.target.value);
                void applyEq(next);
              }}
            />
          </label>
        ))}
      </section>

      <section>
        <h2>Sample rate</h2>
        <select
          data-testid="sample-rate"
          value={sampleRate}
          onChange={(e) => {
            const hz = Number(e.target.value);
            setSampleRate(hz);
            void api("/api/sample-rate", {
              method: "PUT",
              headers: { "content-type": "application/json" },
              body: JSON.stringify({ sample_rate_hz: hz }),
            });
          }}
        >
          <option value={44100}>44100</option>
          <option value={48000}>48000</option>
        </select>
      </section>
    </main>
  );
}

export default App;
