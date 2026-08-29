import { useCallback, useEffect, useMemo, useState } from "react";
import "./App.css";
import {
  API_BASE,
  DEFAULT_PORT,
  type ActiveOutput,
  type OutputInfo,
  type StatusResponse,
} from "@on-air/api-types";
import { Fader } from "./ui/Fader";
import { RateSelect } from "./ui/RateSelect";

const EQ_LABELS = ["60", "250", "1k", "4k", "12k"] as const;

type PairTarget = {
  transport: string;
  id: string;
  name: string;
};

async function api<T>(path: string, init?: RequestInit, timeoutMs = 4000): Promise<T> {
  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), timeoutMs);
  try {
    const response = await fetch(`${API_BASE}${path}`, {
      ...init,
      signal: ctrl.signal,
    });
    if (!response.ok) {
      throw new Error(`${path} ${response.status}`);
    }
    if (response.status === 204) {
      return undefined as T;
    }
    return response.json() as Promise<T>;
  } finally {
    clearTimeout(timer);
  }
}

function prettyInput(name: string): string {
  if (name.startsWith("Discard all samples")) return "Null device";
  if (name.includes("PipeWire Sound Server")) return "PipeWire";
  if (name.startsWith("Default ALSA")) return "Default";
  if (name.includes("analog") && name.endsWith(".monitor")) return "Analog monitor";
  if (name.endsWith(".monitor")) return "System monitor";
  if (name.includes("CS4208 Analog")) return "Built-in analog";
  if (name.includes("HDMI")) return name.replace("HDA Intel HDMI, ", "HDMI ");
  return name;
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
  const [outputSampleRate, setOutputSampleRate] = useState(44100);
  const [inputRates, setInputRates] = useState<number[]>([44100, 48000]);
  const [outputRates, setOutputRates] = useState<number[]>([44100, 48000]);
  const [, setOutputRateTransport] = useState<string | null>(null);
  const [pin, setPin] = useState("");
  const [airplayMode, setAirplayMode] = useState("");
  const [autostart, setAutostart] = useState<boolean | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pairTarget, setPairTarget] = useState<PairTarget | null>(null);
  const [pairPin, setPairPin] = useState("");

  const refresh = useCallback(async () => {
    setError(null);
    const miss = Symbol("miss");
    const settle = async <T,>(p: Promise<T>): Promise<T | typeof miss> => {
      try {
        return await p;
      } catch (e) {
        const msg = String(e);
        if (!msg.includes("AbortError") && !msg.includes("aborted")) {
          setError(msg);
        }
        return miss;
      }
    };
    const [st, ins, outs, actIn, actOut, eq, sr, pairing, ap] = await Promise.all([
      settle(api<StatusResponse>("/api/status")),
      settle(api<{ inputs: string[] }>("/api/inputs")),
      settle(api<{ outputs: OutputInfo[] }>("/api/outputs")),
      settle(api<{ name: string | null; backend: string }>("/api/inputs/active")),
      settle(api<ActiveOutput | null>("/api/outputs/active")),
      settle(api<{ gains_db: [number, number, number, number, number] }>("/api/eq")),
      settle(
        api<{
          sample_rate_hz: number;
          input?: { sample_rate_hz: number; supported_hz: number[] };
          output?: {
            sample_rate_hz: number;
            supported_hz: number[];
            transport?: string;
          };
        }>("/api/sample-rate"),
      ),
      settle(api<{ pin: string }>("/api/pairing/pin")),
      settle(api<{ mode: string }>("/api/airplay/mode")),
    ]);
    if (st !== miss) setStatus(st);
    if (ins !== miss) setInputs(ins.inputs);
    if (outs !== miss) setOutputs(outs.outputs);
    if (actIn !== miss) setActiveInput(actIn.name ?? "");
    if (actOut !== miss) setActiveOutput(actOut);
    if (eq !== miss) setGains(eq.gains_db);
    if (sr !== miss) {
      setSampleRate(sr.input?.sample_rate_hz ?? sr.sample_rate_hz);
      setOutputSampleRate(sr.output?.sample_rate_hz ?? sr.sample_rate_hz);
      if (sr.input?.supported_hz?.length) setInputRates(sr.input.supported_hz);
      if (sr.output?.supported_hz?.length) setOutputRates(sr.output.supported_hz);
      setOutputRateTransport(sr.output?.transport ?? null);
    }
    if (pairing !== miss) setPin(pairing.pin);
    if (ap !== miss) setAirplayMode(ap.mode);
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      setAutostart(await invoke<boolean>("autostart_enabled"));
    } catch {
      setAutostart(null);
    }
  }, []);

  useEffect(() => {
    void refresh();
    const id = setInterval(() => void refresh(), 4000);
    return () => clearInterval(id);
  }, [refresh]);

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

  const chooseOutput = async (output: OutputInfo) => {
    if (output.needs_pair && !output.paired) {
      setPairPin("");
      setPairTarget({
        transport: output.transport,
        id: output.id,
        name: output.name,
      });
      return;
    }
    await activate(output);
  };

  const submitPair = async () => {
    if (!pairTarget) return;
    if (pairTarget.transport === "airplay") {
      await api("/api/airplay/pair", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ device_id: pairTarget.id, pin: pairPin }),
      });
    } else if (pairTarget.transport === "bluetooth") {
      await api("/api/bluetooth/pair", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ id: pairTarget.id }),
      });
    }
    const output = outputs.find(
      (o) => o.transport === pairTarget.transport && o.id === pairTarget.id,
    );
    setPairTarget(null);
    if (output) await activate(output);
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

  const live = Boolean(activeOutput);

  return (
    <main className="app">
      <div className="chassis">
        <header className="mast">
          <div className="brand">
            <h1 className="wordmark" aria-label="on-air">
              <span className="wordmark-ghost" aria-hidden="true">
                ~~~~~~
              </span>
              <span className="wordmark-face">on-air</span>
            </h1>
            <div
              className={`lamp ${live ? "live" : ""}`}
              title={live ? "on air" : "standby"}
              aria-label={live ? "on air" : "standby"}
            >
              <span className="lens" />
            </div>
          </div>
          <div className="meta">
            <span
              className={`meta-ok ${status ? "ok" : ""}`}
              data-testid="core-status"
            >
              {status ? "ok" : "wait"}
            </span>
            <span className="meta-pin">
              pin{" "}
              <span className="pin" data-testid="pairing-pin">
                {pin || "····"}
              </span>
            </span>
            <details className="fold">
              <summary aria-label="more status">▾</summary>
              <div className="fold-body">
                <span>v{status?.version ?? "—"}</span>
                <span>:{DEFAULT_PORT}</span>
                <span data-testid="airplay-mode">{airplayMode || "—"}</span>
                {autostart !== null && (
                  <span data-testid="autostart-hint">
                    {autostart ? "autostart" : "manual"}
                  </span>
                )}
                {airplayMode === "avroute-picker" && (
                  <button
                    className="ghost"
                    data-testid="airplay-picker"
                    onClick={() => {
                      void import("@tauri-apps/api/core").then(({ invoke }) =>
                        invoke("open_airplay_picker"),
                      );
                    }}
                  >
                    AirPlay
                  </button>
                )}
              </div>
            </details>
          </div>
        </header>
        {error && <p className="error">{error}</p>}

        <div className="deck">
          <section className="col">
            <h2 className="kicker">Source</h2>
            <ul className="list" data-testid="input-list">
              {uniqueInputs.length === 0 && <li className="empty">No capture devices</li>}
              {uniqueInputs.map((name) => (
                <li key={name}>
                  <button
                    className={`row ${activeInput === name ? "on" : ""}`}
                    data-testid={`input-${name}`}
                    onClick={() => void pickInput(name)}
                  >
                    <span className="mark" />
                    <span className="name">{prettyInput(name)}</span>
                    <span className="tag">{activeInput === name ? "in" : ""}</span>
                  </button>
                </li>
              ))}
            </ul>
          </section>

          <section className="col">
            <h2 className="kicker">Destination</h2>
            <p data-testid="active-output" hidden>
              {activeOutput
                ? `${activeOutput.transport}: ${activeOutput.device_name}`
                : "none"}
            </p>
            <ul className="list" data-testid="output-list">
              {outputs.length === 0 && (
                <li className="empty">Waiting for a speaker on the LAN</li>
              )}
              {outputs.map((output) => {
                const on =
                  activeOutput?.transport === output.transport &&
                  activeOutput?.device_id === output.id;
                const pair = (output.member_count ?? 1) >= 2 || output.kind === "pair";
                return (
                  <li key={`${output.transport}-${output.id}`}>
                    <button
                      className={`row ${on ? "on" : ""}`}
                      data-testid={`output-${output.transport}-${output.id}`}
                      onClick={() => void chooseOutput(output)}
                    >
                      <span className="mark" />
                      <span className="name">{output.name}</span>
                      <span className="tag">
                        {pair && (
                          <span className="pair-glyph" title="stereo pair">
                            <i />
                            <i />
                          </span>
                        )}
                        {output.needs_pair && !output.paired ? "pin" : output.transport}
                      </span>
                    </button>
                  </li>
                );
              })}
            </ul>
          </section>
        </div>

        <footer className="mix">
          <Fader
            label="level"
            value={volume}
            min={0}
            max={100}
            testId="volume-slider"
            onChange={(v) => void applyVolume(v)}
          />

          <div className="eq-bank">
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
          </div>

          <div className="rates">
            <RateSelect
              label="In"
              value={sampleRate}
              options={inputRates}
              testId="sample-rate"
              onChange={(hz) => {
                setSampleRate(hz);
                void api("/api/sample-rate", {
                  method: "PUT",
                  headers: { "content-type": "application/json" },
                  body: JSON.stringify({ input_hz: hz }),
                });
              }}
            />
            <RateSelect
              label="Out"
              value={outputSampleRate}
              options={outputRates}
              testId="output-sample-rate"
              onChange={(hz) => {
                setOutputSampleRate(hz);
                void api("/api/sample-rate", {
                  method: "PUT",
                  headers: { "content-type": "application/json" },
                  body: JSON.stringify({ output_hz: hz }),
                });
              }}
            />
          </div>
        </footer>
      </div>

      {pairTarget && (
        <div className="sheet" role="dialog">
          <div className="sheet-card">
            <h3>Pair {pairTarget.name}</h3>
            <p>
              {pairTarget.transport === "airplay"
                ? "Enter the HomePod / AirPlay PIN shown on the speaker."
                : "Confirm pairing on the Bluetooth device, then continue."}
            </p>
            {pairTarget.transport === "airplay" && (
              <input
                type="text"
                inputMode="numeric"
                autoFocus
                placeholder="••••"
                value={pairPin}
                onChange={(e) => setPairPin(e.target.value)}
              />
            )}
            <div className="sheet-actions">
              <button type="button" onClick={() => setPairTarget(null)}>
                Cancel
              </button>
              <button className="go" type="button" onClick={() => void submitPair()}>
                Pair &amp; go live
              </button>
            </div>
          </div>
        </div>
      )}
    </main>
  );
}

export default App;
