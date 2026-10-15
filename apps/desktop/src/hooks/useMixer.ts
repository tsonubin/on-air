import type { EqGains } from "@on-air/api-types";
import { createLatestWriteQueue, type LatestWriteQueue } from "@on-air/control-client";
import { useCallback, useEffect, useRef, useState } from "react";
import { type ClientLike, liveClient } from "../lib/client";
import type { CoreSnapshot, RefreshKind } from "./useCoreSnapshot";

export interface UseMixerOptions {
  snapshot: CoreSnapshot;
  client?: ClientLike;
  reportError(err: unknown): void;
  refresh(kind: RefreshKind): Promise<void>;
}

export interface MixerHandle {
  volume: number;
  gains: EqGains;
  inputHz: number;
  outputHz: number;
  inputRates: number[];
  outputRates: number[];
  setVolume(value: number): void;
  setGain(band: number, value: number): void;
  setInputRate(hz: number): void;
  setOutputRate(hz: number): void;
}

const DEFAULT_VOLUME = 50;
const FLAT_EQ: EqGains = [0, 0, 0, 0, 0];
const DEFAULT_RATES = [44100, 48000];

interface Queues {
  volume: LatestWriteQueue<number>;
  eq: LatestWriteQueue<EqGains>;
  input_hz: LatestWriteQueue<number>;
  output_hz: LatestWriteQueue<number>;
}

/**
 * One latest-write queue per control. Each queue sends through the current
 * client and reports a final failure (nothing newer queued) with the last
 * value the core accepted, so the control can roll back to it.
 */
export function useMixer({
  snapshot,
  client = liveClient,
  reportError,
  refresh,
}: UseMixerOptions): MixerHandle {
  const [volume, setVolumeState] = useState(snapshot.volume ?? DEFAULT_VOLUME);
  const [gains, setGainsState] = useState<EqGains>(snapshot.gains ?? FLAT_EQ);
  const [inputHz, setInputHzState] = useState(
    snapshot.sampleRate?.input?.sample_rate_hz ?? snapshot.sampleRate?.sample_rate_hz ?? 44100,
  );
  const [outputHz, setOutputHzState] = useState(
    snapshot.sampleRate?.output?.sample_rate_hz ?? snapshot.sampleRate?.sample_rate_hz ?? 44100,
  );

  // Latest local values, so two quick moves compose instead of clobbering,
  // and the value on screen before an edit is known without a render.
  const latestVolume = useRef(volume);
  const latestGains = useRef<EqGains>(gains);
  const latestInputHz = useRef(inputHz);
  const latestOutputHz = useRef(outputHz);
  const deps = useRef({ client, reportError, refresh });
  deps.current = { client, reportError, refresh };

  const queues = useRef<Queues | null>(null);
  if (queues.current === null) {
    queues.current = {
      volume: createLatestWriteQueue<number>(
        (v) => deps.current.client.setVolume(v),
        (err, _value, committed) => {
          // Nothing newer is queued, so the UI shows the failed value: restore.
          if (committed !== undefined) {
            latestVolume.current = committed;
            setVolumeState(committed);
          }
          deps.current.reportError(err);
        },
      ),
      eq: createLatestWriteQueue<EqGains>(
        (g) => deps.current.client.setEq(g),
        (err, _value, committed) => {
          // Nothing newer is queued: every band goes back to what the core holds.
          if (committed !== undefined) {
            latestGains.current = committed;
            setGainsState(committed);
          }
          deps.current.reportError(err);
        },
      ),
      input_hz: createLatestWriteQueue<number>(
        async (hz) => {
          await deps.current.client.setSampleRate({ input_hz: hz });
          // Changing one side can change the other's supported list.
          await deps.current.refresh("config").catch(() => undefined);
        },
        (err, _value, committed) => {
          if (committed !== undefined) {
            latestInputHz.current = committed;
            setInputHzState(committed);
          }
          deps.current.reportError(err);
        },
      ),
      output_hz: createLatestWriteQueue<number>(
        async (hz) => {
          await deps.current.client.setSampleRate({ output_hz: hz });
          await deps.current.refresh("config").catch(() => undefined);
        },
        (err, _value, committed) => {
          if (committed !== undefined) {
            latestOutputHz.current = committed;
            setOutputHzState(committed);
          }
          deps.current.reportError(err);
        },
      ),
    };
  }
  const q = queues.current;

  // Poll results are accepted for a control only while nothing is being written to it.
  const polledVolume = snapshot.volume;
  useEffect(() => {
    if (polledVolume === null || q.volume.sending()) return;
    latestVolume.current = polledVolume;
    setVolumeState(polledVolume);
  }, [polledVolume, q]);

  const polledGains = snapshot.gains;
  useEffect(() => {
    if (polledGains === null || q.eq.sending()) return;
    latestGains.current = polledGains;
    setGainsState(polledGains);
  }, [polledGains, q]);

  const rate = snapshot.sampleRate;
  const polledInputHz = rate ? rate.input.sample_rate_hz : null;
  const polledOutputHz = rate ? rate.output.sample_rate_hz : null;
  useEffect(() => {
    if (polledInputHz === null || q.input_hz.sending()) return;
    latestInputHz.current = polledInputHz;
    setInputHzState(polledInputHz);
  }, [polledInputHz, q]);
  useEffect(() => {
    if (polledOutputHz === null || q.output_hz.sending()) return;
    latestOutputHz.current = polledOutputHz;
    setOutputHzState(polledOutputHz);
  }, [polledOutputHz, q]);

  // `shown` is what the screen had before the edit: the rollback target
  // until the core accepts a write in this burst.
  const setVolume = useCallback(
    (value: number) => {
      const shown = latestVolume.current;
      latestVolume.current = value;
      setVolumeState(value);
      q.volume.push(value, shown);
    },
    [q],
  );

  const setGain = useCallback(
    (band: number, value: number) => {
      const shown = latestGains.current;
      const next = [...shown] as EqGains;
      next[band] = value;
      latestGains.current = next;
      setGainsState(next);
      q.eq.push(next, shown);
    },
    [q],
  );

  const setRate = useCallback(
    (side: "input" | "output", hz: number) => {
      const queue = side === "input" ? q.input_hz : q.output_hz;
      const setState = side === "input" ? setInputHzState : setOutputHzState;
      const latest = side === "input" ? latestInputHz : latestOutputHz;
      const shown = latest.current;
      latest.current = hz;
      setState(hz);
      queue.push(hz, shown);
    },
    [q],
  );

  const inputRates = rate?.input?.supported_hz?.length ? rate.input.supported_hz : DEFAULT_RATES;
  const outputRates = rate?.output?.supported_hz?.length ? rate.output.supported_hz : DEFAULT_RATES;

  return {
    volume,
    gains,
    inputHz,
    outputHz,
    inputRates,
    outputRates,
    setVolume,
    setGain,
    setInputRate: (hz) => setRate("input", hz),
    setOutputRate: (hz) => setRate("output", hz),
  };
}
