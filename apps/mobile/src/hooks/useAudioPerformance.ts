import type { AudioBenchmark } from "@on-air/api-types";
import { runAudioBenchmark, setCaptureBuffer } from "@on-air/control-client";
import { useRef, useState } from "react";

export function useAudioPerformance(
  base: string | null,
  token: string | null,
  onApplied: () => Promise<void>,
) {
  const identity = `${base ?? ""}|${token ?? ""}`;
  const current = useRef(identity);
  current.current = identity;
  const [result, setResult] = useState<{ identity: string; report: AudioBenchmark } | null>(null);
  const [working, setWorking] = useState<string | null>(null);
  const [failure, setFailure] = useState<{ identity: string; message: string } | null>(null);
  const busy = working === identity;
  const run = async () => {
    if (!base || !token || busy) return;
    setWorking(identity);
    setFailure(null);
    try {
      const report = await runAudioBenchmark(base, token);
      if (current.current === identity) setResult({ identity, report });
    } catch {
      if (current.current === identity)
        setFailure({
          identity,
          message: "The audio check could not complete. Check the host connection and try again.",
        });
    } finally {
      setWorking((previous) => (previous === identity ? null : previous));
    }
  };
  const apply = async (ms: number) => {
    if (!base || !token || busy) return;
    setWorking(identity);
    setFailure(null);
    try {
      await setCaptureBuffer(base, ms, token);
      if (current.current === identity) await onApplied();
    } catch {
      if (current.current === identity)
        setFailure({
          identity,
          message:
            "The capture buffer change could not be confirmed. Check the host connection and current setting.",
        });
    } finally {
      setWorking((previous) => (previous === identity ? null : previous));
    }
  };
  return {
    report: result?.identity === identity ? result.report : null,
    busy,
    error: failure?.identity === identity ? failure.message : null,
    run,
    apply,
  };
}

export type AudioPerformanceHandle = ReturnType<typeof useAudioPerformance>;
