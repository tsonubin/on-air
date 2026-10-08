import type { EqGains } from "@on-air/api-types";
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

export const DEFAULT_VOLUME = 50;
export const FLAT_EQ: EqGains = [0, 0, 0, 0, 0];
const DEFAULT_RATES = [44100, 48000];

type WriteKey = "volume" | "eq" | "input_hz" | "output_hz";

interface Pending<T> {
  value: T;
  send: (value: T) => Promise<void>;
  onFail: (attempted: T, err: unknown) => void;
}

/**
 * Latest-write queue: one request in flight per key, and only the newest
 * value waits behind it. Older pending values are dropped. When a write
 * fails and a newer value is queued, the newer value is sent (it supersedes
 * the failed one); only when nothing newer is pending does `onFail` run, so
 * the UI can roll back to the last committed value.
 */
function useLatestWrite() {
  const inFlight = useRef(new Set<WriteKey>());
  const pending = useRef(new Map<WriteKey, Pending<unknown>>());

  const start = useCallback(<T>(key: WriteKey, job: Pending<T>) => {
    inFlight.current.add(key);
    job.send(job.value).then(
      () => {
        inFlight.current.delete(key);
        const next = pending.current.get(key) as Pending<T> | undefined;
        pending.current.delete(key);
        if (next) start(key, next);
      },
      (err: unknown) => {
        inFlight.current.delete(key);
        const next = pending.current.get(key) as Pending<T> | undefined;
        pending.current.delete(key);
        if (next) start(key, next);
        else job.onFail(job.value, err);
      },
    );
  }, []);

  const write = useCallback(
    <T>(key: WriteKey, job: Pending<T>) => {
      if (inFlight.current.has(key)) {
        pending.current.set(key, job as Pending<unknown>);
        return;
      }
      start(key, job);
    },
    [start],
  );

  const writing = useCallback((key: WriteKey) => inFlight.current.has(key), []);

  return { write, writing };
}

export function useMixer({
  snapshot,
  client = liveClient,
  reportError,
  refresh,
}: UseMixerOptions): MixerHandle {
  const { write, writing } = useLatestWrite();

  const [volume, setVolumeState] = useState(snapshot.volume ?? DEFAULT_VOLUME);
  const [gains, setGainsState] = useState<EqGains>(snapshot.gains ?? FLAT_EQ);
  const [inputHz, setInputHzState] = useState(
    snapshot.sampleRate?.input?.sample_rate_hz ?? snapshot.sampleRate?.sample_rate_hz ?? 44100,
  );
  const [outputHz, setOutputHzState] = useState(
    snapshot.sampleRate?.output?.sample_rate_hz ?? snapshot.sampleRate?.sample_rate_hz ?? 44100,
  );

  // Last value the core confirmed, per key; rollbacks restore from here.
  const committedVolume = useRef(volume);
  const committedGains = useRef<EqGains>(gains);
  const committedInputHz = useRef(inputHz);
  const committedOutputHz = useRef(outputHz);
  // Latest local gains, so two quick fader moves compose instead of clobbering.
  const latestGains = useRef<EqGains>(gains);

  // Poll results are accepted for a key only while nothing is being written to it.
  const polledVolume = snapshot.volume;
  useEffect(() => {
    if (polledVolume === null || writing("volume")) return;
    committedVolume.current = polledVolume;
    setVolumeState(polledVolume);
  }, [polledVolume, writing]);

  const polledGains = snapshot.gains;
  useEffect(() => {
    if (polledGains === null || writing("eq")) return;
    committedGains.current = polledGains;
    latestGains.current = polledGains;
    setGainsState(polledGains);
  }, [polledGains, writing]);

  const rate = snapshot.sampleRate;
  const polledInputHz = rate ? rate.input.sample_rate_hz : null;
  const polledOutputHz = rate ? rate.output.sample_rate_hz : null;
  useEffect(() => {
    if (polledInputHz === null || writing("input_hz")) return;
    committedInputHz.current = polledInputHz;
    setInputHzState(polledInputHz);
  }, [polledInputHz, writing]);
  useEffect(() => {
    if (polledOutputHz === null || writing("output_hz")) return;
    committedOutputHz.current = polledOutputHz;
    setOutputHzState(polledOutputHz);
  }, [polledOutputHz, writing]);

  const setVolume = useCallback(
    (value: number) => {
      setVolumeState(value);
      write<number>("volume", {
        value,
        send: async (v) => {
          await client.setVolume(v);
          committedVolume.current = v;
        },
        onFail: (_attempted, err) => {
          // Nothing newer is queued, so the UI shows the failed value: restore.
          setVolumeState(committedVolume.current);
          reportError(err);
        },
      });
    },
    [client, reportError, write],
  );

  const setGain = useCallback(
    (band: number, value: number) => {
      const next = [...latestGains.current] as EqGains;
      next[band] = value;
      latestGains.current = next;
      setGainsState(next);
      write<EqGains>("eq", {
        value: next,
        send: async (g) => {
          await client.setEq(g);
          committedGains.current = g;
        },
        onFail: (_attempted, err) => {
          // Nothing newer is queued: every band goes back to what the core holds.
          latestGains.current = committedGains.current;
          setGainsState(committedGains.current);
          reportError(err);
        },
      });
    },
    [client, reportError, write],
  );

  const setRate = useCallback(
    (side: "input" | "output", hz: number) => {
      const key: WriteKey = side === "input" ? "input_hz" : "output_hz";
      const setState = side === "input" ? setInputHzState : setOutputHzState;
      const committed = side === "input" ? committedInputHz : committedOutputHz;
      setState(hz);
      write<number>(key, {
        value: hz,
        send: async (v) => {
          await client.setSampleRate(side === "input" ? { input_hz: v } : { output_hz: v });
          committed.current = v;
          // Changing one side can change the other's supported list.
          await refresh("config").catch(() => undefined);
        },
        onFail: (_attempted, err) => {
          setState(committed.current);
          reportError(err);
        },
      });
    },
    [client, refresh, reportError, write],
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
