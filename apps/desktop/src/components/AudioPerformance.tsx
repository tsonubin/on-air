import { API_BASE, type AudioBenchmark } from "@on-air/api-types";
import {
  AUDIO_CHECK_DESCRIPTION,
  AUDIO_CHECK_OVER_BUDGET,
  audioBenchmarkSummary,
  CAPTURE_BUFFER_OPTIONS,
  captureBufferLabel,
  runAudioBenchmark,
  setCaptureBuffer,
} from "@on-air/control-client";
import { useState } from "react";
import { Button } from "../ui/Button";

export function AudioPerformance({
  bufferMs,
  disabled,
  onApplied,
}: {
  bufferMs?: number;
  disabled: boolean;
  onApplied(): void;
}) {
  const [report, setReport] = useState<AudioBenchmark | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<number | null>(null);
  const available = bufferMs !== undefined;
  const check = async () => {
    setBusy(true);
    setError(null);
    try {
      setReport(await runAudioBenchmark(API_BASE));
    } catch {
      setError("The audio check could not complete. Check the host connection and try again.");
    } finally {
      setBusy(false);
    }
  };
  const apply = async (ms: number) => {
    setBusy(true);
    setError(null);
    try {
      await setCaptureBuffer(API_BASE, ms);
      setSelected(null);
      onApplied();
    } catch {
      setError(
        "The capture buffer change could not be confirmed. Check the host connection and current setting.",
      );
    } finally {
      setBusy(false);
    }
  };
  return (
    <details className="mx-3 mb-2 rounded border border-line p-2 text-sm text-steel">
      <summary className="cursor-pointer">Audio performance</summary>
      <p>{AUDIO_CHECK_DESCRIPTION}</p>
      {!available && <p>Update the on-air core to use these settings.</p>}
      <div className="flex flex-wrap items-center gap-2">
        <label>
          Requested capture buffer{" "}
          <select
            aria-label="Capture buffer"
            className="rounded border border-line bg-face-2 p-1 text-ink"
            value={selected ?? bufferMs ?? 0}
            disabled={disabled || busy || !available}
            onChange={(e) => setSelected(Number(e.target.value))}
          >
            {CAPTURE_BUFFER_OPTIONS.map((ms) => (
              <option key={ms} value={ms}>
                {captureBufferLabel(ms)}
              </option>
            ))}
          </select>
        </label>
        <Button
          disabled={disabled || busy || !available || selected === null}
          onClick={() => void apply(selected ?? bufferMs ?? 0)}
        >
          Apply buffer
        </Button>
        <Button
          disabled={disabled || busy || !available}
          aria-busy={busy}
          onClick={() => void check()}
        >
          {busy ? "Working…" : "Run audio check"}
        </Button>
      </div>
      {report && (
        <>
          <p>{audioBenchmarkSummary(report)}</p>
          {!report.within_buffer_budget && <p>{AUDIO_CHECK_OVER_BUDGET}</p>}
          <Button
            disabled={disabled || busy}
            onClick={() => void apply(report.recommended_buffer_ms)}
          >
            Apply recommendation ({report.recommended_buffer_ms} ms)
          </Button>
        </>
      )}
      {error && <p role="alert">{error}</p>}
    </details>
  );
}
