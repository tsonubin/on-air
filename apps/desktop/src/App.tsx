import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import "./App.css";
import {
  type ActiveOutput,
  API_BASE,
  DEFAULT_PORT,
  type OutputInfo,
  type StatusResponse,
  type WsEvent,
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
  const [gains, setGains] = useState<[number, number, number, number, number]>([0, 0, 0, 0, 0]);
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
  const [pairing, setPairing] = useState(false);
  const pairPinInput = useRef<HTMLInputElement>(null);
  const refreshInFlight = useRef<Promise<void> | null>(null);
  const refreshQueued = useRef(false);

  const refresh = useCallback((): Promise<void> => {
    if (refreshInFlight.current) {
      refreshQueued.current = true;
      return refreshInFlight.current;
    }
    const run = (async () => {
      do {
        refreshQueued.current = false;
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
      } while (refreshQueued.current);
    })();
    refreshInFlight.current = run;
    void run.finally(() => {
      if (refreshInFlight.current === run) refreshInFlight.current = null;
    });
    return run;
  }, []);

  useEffect(() => {
    const refreshIfVisible = () => {
      if (!document.hidden) void refresh();
    };
    refreshIfVisible();
    const id = setInterval(refreshIfVisible, 15000);
    document.addEventListener("visibilitychange", refreshIfVisible);
    const ws = new WebSocket(`${API_BASE.replace(/^http/i, "ws")}/api/ws`);
    ws.onmessage = (event) => {
      try {
        const message = JSON.parse(String(event.data)) as WsEvent;
        if (message.type !== "LevelMeter") refreshIfVisible();
      } catch {
        // The interval remains the recovery path for malformed events.
      }
    };
    return () => {
      clearInterval(id);
      document.removeEventListener("visibilitychange", refreshIfVisible);
      ws.close();
    };
  }, [refresh]);

  useEffect(() => {
    void import("@tauri-apps/api/core")
      .then(({ invoke }) => invoke<boolean>("autostart_enabled"))
      .then(setAutostart)
      .catch(() => setAutostart(null));
  }, []);

  useEffect(() => {
    if (pairTarget?.transport === "airplay") {
      pairPinInput.current?.focus();
    }
  }, [pairTarget]);

  const uniqueInputs = useMemo(() => {
    const seen = new Set<string>();
    return inputs.filter((name) => {
      const label = prettyInput(name);
      if (seen.has(label)) return false;
      seen.add(label);
      return true;
    });
  }, [inputs]);

  const perform = async (action: () => Promise<void>): Promise<boolean> => {
    setError(null);
    try {
      await action();
      return true;
    } catch (err) {
      setError(String(err));
      return false;
    }
  };

  const pickInput = async (name: string) => {
    await perform(async () => {
      await api("/api/inputs/active", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ name }),
      });
      await refresh();
    });
  };

  const activate = async (output: OutputInfo): Promise<boolean> =>
    perform(async () => {
      await api("/api/outputs/active", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ transport: output.transport, device_id: output.id }),
      });
      await refresh();
    });

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
    if (!pairTarget || pairing) return;
    setPairing(true);
    const paired = await perform(async () => {
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
    });
    setPairing(false);
    if (!paired) return;
    const output = outputs.find(
      (o) => o.transport === pairTarget.transport && o.id === pairTarget.id,
    );
    if (!output || (await activate(output))) setPairTarget(null);
  };

  const applyVolume = async (value: number) => {
    const previous = volume;
    setVolume(value);
    const updated = await perform(() =>
      api("/api/outputs/active/volume", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ volume: value }),
      }),
    );
    if (!updated) setVolume(previous);
  };

  const applyEq = async (next: [number, number, number, number, number]) => {
    const previous = gains;
    setGains(next);
    const updated = await perform(() =>
      api("/api/eq", {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ gains_db: next }),
      }),
    );
    if (!updated) setGains(previous);
  };

  const applyRate = async (kind: "input" | "output", hz: number) => {
    const previous = kind === "input" ? sampleRate : outputSampleRate;
    if (kind === "input") setSampleRate(hz);
    else setOutputSampleRate(hz);
    const updated = await perform(() =>
      api("/api/sample-rate", {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(kind === "input" ? { input_hz: hz } : { output_hz: hz }),
      }),
    );
    if (!updated) {
      if (kind === "input") setSampleRate(previous);
      else setOutputSampleRate(previous);
    } else {
      await refresh();
    }
  };

  const serviceAvailable = status?.service_enabled !== false;
  const live = serviceAvailable && Boolean(activeOutput);
  const lampMode = live ? "live" : error || !status || !serviceAvailable ? "warn" : "ok";
  const lampLabel = live
    ? "on air"
    : !serviceAvailable
      ? "service paused"
      : error || !status
        ? "problem"
        : "ok";

  const wordmarkTone =
    lampMode === "live" ? "wordmark-live" : lampMode === "warn" ? "wordmark-warn" : "wordmark-ok";
  const wordmarkFace =
    lampMode === "live"
      ? "wordmark-live-face"
      : lampMode === "warn"
        ? "wordmark-warn-face"
        : "wordmark-ok-face";

  return (
    <main className="flex h-full min-h-full flex-col gap-2.5 overflow-auto p-[clamp(10px,2vw,18px)]">
      <div className="chassis flex min-h-0 flex-1 flex-col rounded-[10px] border border-[#3a342a]">
        <header className="flex flex-wrap items-center justify-between gap-2.5 gap-x-[18px] border-b border-[#2e2a24] px-4 py-3 max-[720px]:p-3">
          <div className="flex items-center gap-3.5">
            <h1
              className={`wordmark relative m-0 inline-flex items-center rounded-sm px-3 py-1.5 text-[18px] font-normal [@media(max-height:560px)]:px-2.5 [@media(max-height:560px)]:py-1 [@media(max-height:560px)]:text-base ${wordmarkTone}`}
              aria-label={lampLabel}
              title={lampLabel}
              data-testid="core-status"
            >
              <span
                className={`wordmark-gel pointer-events-none absolute inset-[3px] z-1 rounded-sm opacity-55 ${lampMode === "live" ? "opacity-70" : ""}`}
                aria-hidden="true"
              />
              <span className={`relative z-2 ${wordmarkFace}`}>ONAIR</span>
              <span className="sr-only">{status ? "ok" : "wait"}</span>
            </h1>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <div className="flex items-center gap-1.5 rounded border border-[#3a342a] bg-linear-to-b from-[#2c281f] via-face to-[#12100c] px-2 py-1 shadow-[inset_0_1px_0_#4a4338,0_1px_2px_#000]">
              <span className="font-mono text-[8px] tracking-[0.18em] text-steel-dim uppercase">
                pin
              </span>
              <span className="relative inline-grid rounded-sm bg-[#070605] px-1.5 py-0.5 font-led text-[13px] leading-none tracking-[0.1em] shadow-[inset_0_1px_3px_#000]">
                <span
                  className="col-start-1 row-start-1 text-[#1c1812] select-none"
                  aria-hidden="true"
                >
                  888888
                </span>
                <span
                  className="col-start-1 row-start-1 text-amber [text-shadow:0_0_4px_rgba(224,162,75,0.8),0_0_10px_rgba(224,162,75,0.35)]"
                  data-testid="pairing-pin"
                >
                  {pin || "····"}
                </span>
              </span>
            </div>
            <details className="relative">
              <summary
                aria-label="more status"
                className="hatch-knob size-[18px] cursor-pointer rounded-full border border-[#1a1814] hover:brightness-110"
              />
              <div className="absolute top-[calc(100%+8px)] right-0 z-30 flex min-w-[7.5rem] flex-col gap-1.5 rounded-b bg-linear-to-b from-[#2a261f] to-[#16140f] px-2.5 py-2 font-mono text-[11px] text-ink shadow-[inset_0_1px_0_#4a4338,0_10px_22px_rgba(0,0,0,0.5)] border border-[#3a342a]">
                <span>v{status?.version ?? "—"}</span>
                <span>{serviceAvailable ? "service on" : "service paused"}</span>
                <span>:{DEFAULT_PORT}</span>
                <span data-testid="airplay-mode">{airplayMode || "—"}</span>
                {autostart !== null && (
                  <span data-testid="autostart-hint">{autostart ? "autostart" : "manual"}</span>
                )}
                {airplayMode === "avroute-picker" && (
                  <button
                    type="button"
                    className="cursor-pointer rounded-sm border border-[#3a342a] bg-[#141210] px-2 py-0.5 hover:border-amber hover:text-amber"
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
        {error && (
          <p className="mx-3 my-0 rounded bg-[#3a1410] px-2.5 py-2 text-xs text-[#f0b4ac]">
            {error}
          </p>
        )}

        <div className="grid min-h-0 flex-1 grid-cols-1 grid-rows-[minmax(0,1fr)_minmax(0,1fr)] overflow-hidden min-[721px]:grid-cols-[minmax(0,1fr)_minmax(0,1.15fr)] min-[721px]:grid-rows-[minmax(0,1fr)]">
          <section className="flex min-h-0 min-w-0 flex-col overflow-hidden p-3">
            <h2 className="mb-2 shrink-0 font-mono text-[10px] tracking-[0.22em] text-steel-dim uppercase">
              Source
            </h2>
            <ul
              className="flex min-h-0 flex-1 list-none flex-col gap-1.5 overflow-x-hidden overflow-y-auto overscroll-contain p-0 pr-0.5 m-0"
              data-testid="input-list"
            >
              {uniqueInputs.length === 0 && (
                <li className="px-1 py-2.5 text-xs text-steel-dim">No capture devices</li>
              )}
              {uniqueInputs.map((name) => (
                <li key={name}>
                  <button
                    type="button"
                    className={`device-row grid w-full cursor-pointer grid-cols-[18px_minmax(0,1fr)_auto] items-center gap-2 rounded-md border border-[#322e26] px-2.5 py-2 text-left hover:border-[#4a4336] ${activeInput === name ? "device-row-on" : ""}`}
                    data-testid={`input-${name}`}
                    onClick={() => void pickInput(name)}
                  >
                    <span
                      className={`size-[9px] rounded-full border border-steel-dim ${activeInput === name ? "border-live bg-live" : ""}`}
                    />
                    <span className="truncate whitespace-nowrap">{prettyInput(name)}</span>
                    <span className="flex items-center gap-1.5 font-mono text-[9px] tracking-[0.14em] text-steel-dim uppercase">
                      {activeInput === name ? "in" : ""}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          </section>

          <section className="flex min-h-0 min-w-0 flex-col overflow-hidden border-t border-[#2e2a24] p-3 min-[721px]:border-t-0 min-[721px]:border-l">
            <div className="mb-2 flex shrink-0 items-center justify-between gap-2">
              <h2 className="m-0 font-mono text-[10px] tracking-[0.22em] text-steel-dim uppercase">
                Destination
              </h2>
              {airplayMode === "avroute-picker" && (
                <button
                  type="button"
                  className="cursor-pointer rounded-sm border border-[#3a342a] bg-[#141210] px-2 py-1 font-mono text-[10px] hover:border-amber hover:text-amber"
                  onClick={() => {
                    void import("@tauri-apps/api/core")
                      .then(({ invoke }) => invoke("open_airplay_picker"))
                      .catch((err) => setError(String(err)));
                  }}
                >
                  AirPlay picker
                </button>
              )}
            </div>
            <p data-testid="active-output" hidden>
              {activeOutput ? `${activeOutput.transport}: ${activeOutput.device_name}` : "none"}
            </p>
            <ul
              className="flex min-h-0 flex-1 list-none flex-col gap-1.5 overflow-x-hidden overflow-y-auto overscroll-contain p-0 pr-0.5 m-0"
              data-testid="output-list"
            >
              {outputs.length === 0 && (
                <li className="px-1 py-2.5 text-xs text-steel-dim">
                  Waiting for a speaker on the LAN
                </li>
              )}
              {outputs.map((output) => {
                const on =
                  activeOutput?.transport === output.transport &&
                  activeOutput?.device_id === output.id;
                const pair = (output.member_count ?? 1) >= 2 || output.kind === "pair";
                const pickerOnly =
                  output.transport === "airplay" && airplayMode === "avroute-picker";
                return (
                  <li key={`${output.transport}-${output.id}`}>
                    <button
                      type="button"
                      className={`device-row grid w-full cursor-pointer grid-cols-[18px_minmax(0,1fr)_auto] items-center gap-2 rounded-md border border-[#322e26] px-2.5 py-2 text-left hover:border-[#4a4336] disabled:cursor-not-allowed disabled:opacity-50 ${on ? "device-row-on" : ""}`}
                      data-testid={`output-${output.transport}-${output.id}`}
                      onClick={() => void chooseOutput(output)}
                      disabled={pickerOnly}
                    >
                      <span
                        className={`size-[9px] rounded-full border border-steel-dim ${on ? "border-live bg-live" : ""}`}
                      />
                      <span className="truncate whitespace-nowrap">{output.name}</span>
                      <span className="flex items-center gap-1.5 font-mono text-[9px] tracking-[0.14em] text-steel-dim uppercase">
                        {pair && (
                          <span className="inline-flex gap-0.5" title="stereo pair">
                            <i className="block h-2.5 w-1.5 rounded-[1px_3px_3px_1px] border border-amber" />
                            <i className="block h-2.5 w-1.5 rounded-[1px_3px_3px_1px] border border-amber" />
                          </span>
                        )}
                        {pickerOnly
                          ? "picker"
                          : output.needs_pair && !output.paired
                            ? "pin"
                            : output.transport}
                      </span>
                    </button>
                  </li>
                );
              })}
            </ul>
          </section>
        </div>

        <footer className="grid shrink-0 grid-cols-1 items-end justify-items-center gap-4 border-t border-[#2e2a24] bg-[#151310] px-4 py-3 min-[561px]:grid-cols-[auto_minmax(0,1fr)_minmax(9.5rem,11rem)] min-[561px]:justify-items-stretch [@media(max-height:560px)]:gap-2.5 [@media(max-height:560px)]:px-3 [@media(max-height:560px)]:py-2">
          <Fader
            label="level"
            value={volume}
            min={0}
            max={100}
            testId="volume-slider"
            onChange={(v) => void applyVolume(v)}
          />

          <div className="flex min-w-0 items-end justify-center gap-3">
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
          </div>

          <div className="flex w-full min-w-[11rem] flex-col justify-end gap-2">
            <RateSelect
              label="In"
              value={sampleRate}
              options={inputRates}
              testId="sample-rate"
              onChange={(hz) => void applyRate("input", hz)}
            />
            <RateSelect
              label="Out"
              value={outputSampleRate}
              options={outputRates}
              testId="output-sample-rate"
              onChange={(hz) => void applyRate("output", hz)}
            />
          </div>
        </footer>
      </div>

      {pairTarget && (
        <div
          className="fixed inset-0 z-20 flex items-end justify-center bg-[rgba(8,7,6,0.72)] p-4"
          role="dialog"
        >
          <div className="w-full max-w-[420px] rounded-t-[10px] rounded-b bg-face-2 border border-[#3a342a] p-4 shadow-[0_-12px_40px_rgba(0,0,0,0.5)]">
            <h3 className="mt-0 mb-1.5 font-sign text-xl">Pair {pairTarget.name}</h3>
            <p className="mt-0 mb-3 text-xs text-steel-dim">
              {pairTarget.transport === "airplay"
                ? "Only if this speaker shows a code (Home app or Apple TV). HomePod mini has no screen and usually has no PIN."
                : "Confirm pairing on the Bluetooth device, then continue."}
            </p>
            {pairTarget.transport === "airplay" && (
              <input
                ref={pairPinInput}
                type="text"
                inputMode="numeric"
                placeholder="••••"
                value={pairPin}
                onChange={(e) => setPairPin(e.target.value)}
                className="h-auto w-full rounded border border-[#3a342a] bg-well p-2.5 font-mono text-lg tracking-[0.28em]"
              />
            )}
            <div className="mt-3 flex gap-2">
              <button
                type="button"
                disabled={pairing}
                className="flex-1 cursor-pointer rounded border border-[#3a342a] bg-[#141210] py-2.5"
                onClick={() => setPairTarget(null)}
              >
                Cancel
              </button>
              <button
                className="flex-1 cursor-pointer rounded border border-[#6a2a22] bg-[#3a1814] py-2.5 text-[#ffd4cc]"
                type="button"
                disabled={pairing}
                onClick={() => void submitPair()}
              >
                {pairing ? "Pairing…" : "Pair & go live"}
              </button>
            </div>
          </div>
        </div>
      )}
    </main>
  );
}

export default App;
